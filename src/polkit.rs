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

/// What the agent asks of GTK's thread.
enum Up {
    Ask(Ask),
    Cancel,
}

pub fn start(prompts: &Rc<Prompts>) {
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
    conn.call_method(
        Some("org.freedesktop.PolicyKit1"),
        "/org/freedesktop/PolicyKit1/Authority",
        Some("org.freedesktop.PolicyKit1.Authority"),
        "RegisterAuthenticationAgent",
        &(subject, locale, AGENT_PATH),
    )
    .await?;
    // served for as long as the program runs
    std::future::pending::<()>().await;
    Ok(())
}
