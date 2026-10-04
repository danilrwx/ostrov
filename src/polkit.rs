//! The polkit agent, in place of lxpolkit: an app asking to do something as root (pkexec, a package manager, a
//! system setting) has ostrov ask for the password, in its own look (prompt.rs). Registered with polkit for this
//! session on the system bus; the password checked by polkit's own helper, through the socket systemd keeps for
//! it (/run/polkit/agent-helper.socket): the user's name and the request's cookie down it, then PAM's
//! conversation line by line (PAM_PROMPT_ECHO_OFF Password:, ..., SUCCESS or FAILURE). The password goes from the
//! dialog to the helper and nowhere else.

use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use zbus::zvariant::{OwnedValue, Value};

use crate::i18n::{fill, t};
use crate::prompt::{Ask, Kind, Prompts};

const AGENT_PATH: &str = "/dev/ostrov/PolkitAgent";

/// The agents ostrov knows by their programs' names, with the name they go by: the one answering when ostrov is
/// refused (polkit takes one agent a session, the first registered, and tells no other which).
const OTHERS: &[(&str, &str)] = &[
    ("lxpolkit", "lxpolkit"),
    ("hyprpolkitagent", "hyprpolkitagent"),
    ("polkit-gnome-au", "polkit-gnome"),
    ("polkit-kde-auth", "polkit-kde-agent"),
    ("polkit-mate-aut", "mate-polkit"),
    ("xfce-polkit", "xfce-polkit"),
];

/// Who asks the session's passwords, as far as ostrov knows.
#[derive(Clone, Debug, PartialEq)]
pub enum Answering {
    /// not found out yet (polkit not answered), or polkit not there
    Unknown,
    Ostrov,
    /// another agent, by name ("" one ostrov does not know)
    Other(String),
    /// [polkit] agent = false
    Off,
}

static ANSWERING: std::sync::Mutex<Answering> = std::sync::Mutex::new(Answering::Unknown);

pub fn answering() -> Answering {
    ANSWERING.lock().map(|a| a.clone()).unwrap_or(Answering::Unknown)
}

fn set(a: Answering) {
    if let Ok(mut now) = ANSWERING.lock() {
        *now = a;
    }
}

/// The system's XDG autostart of a known agent (name as OTHERS gives it), the way it gets into a session it was not
/// made for (Hyprland under uwsm runs them).
pub fn autostart(name: &str) -> Option<std::path::PathBuf> {
    let (comm, _) = OTHERS.iter().find(|(_, n)| *n == name)?;
    std::fs::read_dir("/etc/xdg/autostart").ok()?.flatten().map(|e| e.path()).find(|p| {
        std::fs::read_to_string(p).is_ok_and(|s| {
            s.lines().filter_map(|l| l.strip_prefix("Exec=")).any(|e| {
                let prog = e.split_whitespace().next().unwrap_or("");
                prog.rsplit('/').next().is_some_and(|b| b.starts_with(comm))
            })
        })
    })
}

/// That autostart kept out of Hyprland for this user: a copy in ~/.config/autostart (which XDG takes over the
/// system's) with Hyprland among its NotShowIn, the other desktops left as they were.
pub fn keep_out(system: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let text = std::fs::read_to_string(system)?;
    let to = crate::hub::home().join(".config/autostart").join(system.file_name().unwrap_or_default());
    std::fs::create_dir_all(to.parent().unwrap_or(&to))?;
    std::fs::write(&to, without_hyprland(&text))?;
    Ok(to)
}

/// A desktop entry's text with Hyprland in its NotShowIn.
fn without_hyprland(text: &str) -> String {
    let mut seen = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| match l.strip_prefix("NotShowIn=") {
            Some(v) => {
                seen = true;
                format!("NotShowIn={}Hyprland;", if v.is_empty() || v.ends_with(';') { v.to_string() } else { format!("{v};") })
            }
            None => l.to_string(),
        })
        .collect();
    if !seen {
        out.push("NotShowIn=Hyprland;".into());
    }
    out.join("\n") + "\n"
}

/// The other agent running, by /proc's names of this user's processes.
fn other() -> String {
    let uid = std::fs::metadata("/proc/self").map(|m| std::os::unix::fs::MetadataExt::uid(&m)).ok();
    let comms = std::fs::read_dir("/proc").into_iter().flatten().flatten().filter(|e| {
        uid.is_some() && e.metadata().is_ok_and(|m| Some(std::os::unix::fs::MetadataExt::uid(&m)) == uid)
    });
    let names: Vec<String> =
        comms.filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok()).map(|c| c.trim().to_string()).collect();
    OTHERS.iter().find(|(comm, _)| names.iter().any(|n| n == comm)).map_or(String::new(), |(_, name)| name.to_string())
}

/// What the agent asks of GTK's thread.
enum Up {
    Ask(Ask),
    Cancel,
}

/// The agent, unless [polkit] says another's; refused for another agent there first, ostrov leaves it be.
pub fn start(prompts: &Rc<Prompts>) {
    if !crate::config::load().polkit.agent {
        set(Answering::Off);
        return;
    }
    let (tx, rx) = async_channel::unbounded::<Up>();
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
        rt.block_on(async move {
            if let Err(e) = serve(tx).await {
                eprintln!("ostrov: polkit: {e}");
            }
        });
    });
    let prompts = prompts.clone();
    glib::spawn_future_local(async move {
        while let Ok(up) = rx.recv().await {
            match up {
                Up::Ask(a) => prompts.ask(a),
                Up::Cancel => prompts.cancel(),
            }
        }
    });
}

struct Agent {
    up: async_channel::Sender<Up>,
}

/// A name from /etc/passwd by uid; None for one not there.
fn user_name(uid: u32) -> Option<String> {
    std::fs::read_to_string("/etc/passwd")
        .ok()?
        .lines()
        .find_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            (f.get(2)? == &uid.to_string()).then(|| f[0].to_string())
        })
}

/// The user to authenticate as, of those polkit will take: this one if it is among them, else the first.
fn identity(ids: &[(String, HashMap<String, OwnedValue>)]) -> Option<String> {
    let me = std::env::var("USER").unwrap_or_default();
    let names: Vec<String> = ids
        .iter()
        .filter(|(kind, _)| kind == "unix-user")
        .filter_map(|(_, d)| u32::try_from(d.get("uid")?.clone()).ok().and_then(user_name))
        .collect();
    names.iter().find(|n| **n == me).or(names.first()).cloned()
}

impl Agent {
    /// The user asked, the helper told, again on a wrong password, until it takes one or the user says no.
    async fn authenticate(&self, user: &str, cookie: &str, message: &str, icon: &str) -> Result<(), String> {
        let mut error = String::new();
        loop {
            let s = tokio::net::UnixStream::connect("/run/polkit/agent-helper.socket").await.map_err(|e| e.to_string())?;
            let (r, mut w) = s.into_split();
            w.write_all(format!("{user}\n{cookie}\n").as_bytes()).await.map_err(|e| e.to_string())?;
            let mut lines = BufReader::new(r).lines();
            let mut info = String::new();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(prompt) = line.strip_prefix("PAM_PROMPT_ECHO_OFF ").or(line.strip_prefix("PAM_PROMPT_ECHO_ON ")) {
                    let (reply, answer) = async_channel::bounded(1);
                    let title = if prompt.trim().trim_end_matches(':').is_empty() { t("Password") } else { prompt.trim().trim_end_matches(':') };
                    let text = if info.is_empty() { message.to_string() } else { format!("{message}\n{info}") };
                    let mut ask = Ask::new(icon, &fill(t("{} for {}"), &[&title, &user]), &text, Kind::Secret, reply);
                    ask.error = std::mem::take(&mut error);
                    self.up.send(Up::Ask(ask)).await.map_err(|e| e.to_string())?;
                    let Some(password) = answer.recv().await.ok().flatten() else {
                        return Err("cancelled".into());
                    };
                    w.write_all(format!("{password}\n").as_bytes()).await.map_err(|e| e.to_string())?;
                } else if let Some(m) = line.strip_prefix("PAM_ERROR_MSG ").or(line.strip_prefix("PAM_TEXT_INFO ")) {
                    info = m.to_string();
                } else if line == "SUCCESS" {
                    return Ok(());
                } else if line == "FAILURE" {
                    break;
                }
            }
            error = t("Wrong password, try again").into();
        }
    }
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    async fn begin_authentication(
        &self,
        _action_id: String,
        message: String,
        icon_name: String,
        _details: HashMap<String, String>,
        cookie: String,
        identities: Vec<(String, HashMap<String, OwnedValue>)>,
    ) -> zbus::fdo::Result<()> {
        let user = identity(&identities).ok_or(zbus::fdo::Error::Failed("no user to authenticate as".into()))?;
        self.authenticate(&user, &cookie, &message, &icon_name).await.map_err(|e| {
            if e == "cancelled" {
                zbus::fdo::Error::Failed("cancelled by the user".into())
            } else {
                zbus::fdo::Error::Failed(e)
            }
        })
    }

    async fn cancel_authentication(&self, _cookie: String) {
        let _ = self.up.send(Up::Cancel).await;
    }
}

/// The agent on the system bus, registered for this session.
async fn serve(up: async_channel::Sender<Up>) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    conn.object_server().at(AGENT_PATH, Agent { up }).await?;
    // this session's id, as logind knows it
    let reply = conn
        .call_method(Some("org.freedesktop.login1"), "/org/freedesktop/login1", Some("org.freedesktop.login1.Manager"), "GetSession", &("auto",))
        .await?;
    let path: zbus::zvariant::OwnedObjectPath = reply.body().deserialize()?;
    let reply = conn
        .call_method(Some("org.freedesktop.login1"), path.as_str(), Some("org.freedesktop.DBus.Properties"), "Get", &("org.freedesktop.login1.Session", "Id"))
        .await?;
    let id: OwnedValue = reply.body().deserialize()?;
    let id = String::try_from(id).unwrap_or_default();
    let subject = ("unix-session", HashMap::from([("session-id", Value::from(id.as_str()))]));
    let locale = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into());
    let registered = conn
        .call_method(
            Some("org.freedesktop.PolicyKit1"),
            "/org/freedesktop/PolicyKit1/Authority",
            Some("org.freedesktop.PolicyKit1.Authority"),
            "RegisterAuthenticationAgent",
            &(subject, locale, AGENT_PATH),
        )
        .await;
    match registered {
        Ok(_) => set(Answering::Ostrov),
        // another agent first: its dialogs, not an error of ostrov's
        Err(e) if e.to_string().contains("already exists") => {
            set(Answering::Other(other()));
            return Ok(());
        }
        Err(e) => return Err(e),
    }
    // served for as long as the program runs
    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyprland_added_to_not_show_in() {
        let a = without_hyprland("[Desktop Entry]\nExec=lxpolkit\nNotShowIn=GNOME;KDE;\n");
        assert!(a.contains("NotShowIn=GNOME;KDE;Hyprland;"));
        assert!(without_hyprland("[Desktop Entry]\nExec=x\n").ends_with("NotShowIn=Hyprland;\n"));
    }
}

