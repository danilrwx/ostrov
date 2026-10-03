//! The login screen, greetd's greeter in tuigreet's place: `ostrov greet`, under cage on greetd's vt
//! (etc/greetd/config.toml). None of the desktop, only the lock screen's look: black, the time large, the date, then
//! the user (the last one logged in, else the first human in /etc/passwd), the password, the session (the
//! .desktop files of wayland-sessions and xsessions, an X one through startx as tuigreet's --xsession-wrapper did)
//! and the power buttons. The user and the session are remembered in greetd's own cache (STATE), the greeter user
//! being the one who writes it. greetd is spoken to over $GREETD_SOCK: a u32 length in native order, then JSON.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde::Deserialize;
use serde_json::{json, Value};

/// The last login's user and session, a line each; its directory the greeter user's (dotfiles' install makes it).
const STATE: &str = "/var/cache/ostrov-greet/state";

/// The greeter: a GApplication of its own, not unique, for there is no session bus to be one on.
pub fn run() -> glib::ExitCode {
    let app = gtk4::Application::builder()
        .application_id("dev.danil.ostrov.greet")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(build);
    // `greet` is ostrov's word, not a file for GApplication to open
    app.run_with_args(&["ostrov"])
}

fn build(app: &gtk4::Application) {
    crate::style::load();
    let win = gtk4::ApplicationWindow::new(app);
    win.add_css_class("lock");
    win.add_css_class("greet");
    // cage keeps its one window full screen anyway; the layer, where there is one, keeps the keyboard too
    if gtk4_layer_shell::is_supported() {
        win.init_layer_shell();
        win.set_layer(Layer::Overlay);
        win.set_namespace(Some("ostrov-greet"));
        for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            win.set_anchor(e, true);
        }
        win.set_exclusive_zone(-1);
        win.set_keyboard_mode(KeyboardMode::Exclusive);
    } else {
        win.fullscreen();
    }

    let (last_user, last_session) = remembered();
    let sessions = Rc::new(sessions());

    let col = gtk4::Box::new(Orientation::Vertical, 14);
    col.set_halign(Align::Center);
    col.set_valign(Align::Center);
    crate::lock::clock(&col);
    let user = gtk4::Entry::new();
    user.add_css_class("lock-entry");
    EditableExt::set_alignment(&user, 0.5);
    user.set_placeholder_text(Some("user"));
    user.set_text(&last_user.or_else(first_user).unwrap_or_default());
    let password = gtk4::PasswordEntry::new();
    password.add_css_class("lock-entry");
    password.set_alignment(0.5);
    password.set_placeholder_text(Some("password"));
    let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
    let session = gtk4::DropDown::from_strings(&names);
    session.add_css_class("lock-entry");
    if let Some(i) = sessions.iter().position(|s| Some(&s.name) == last_session.as_ref()) {
        session.set_selected(i as u32);
    }
    let error = gtk4::Label::new(None);
    error.add_css_class("error");
    for w in [user.upcast_ref::<gtk4::Widget>(), password.upcast_ref(), session.upcast_ref(), error.upcast_ref()] {
        col.append(w);
    }

    let power = gtk4::Box::new(Orientation::Horizontal, 8);
    power.add_css_class("greet-power");
    power.set_halign(Align::End);
    power.set_valign(Align::End);
    power.set_margin_end(24);
    power.set_margin_bottom(24);
    for (icon, verb) in [("system-reboot-symbolic", "reboot"), ("system-shutdown-symbolic", "poweroff")] {
        let b = gtk4::Button::from_icon_name(icon);
        b.set_tooltip_text(Some(verb));
        b.connect_clicked(move |_| {
            let _ = std::process::Command::new("systemctl").arg(verb).spawn();
        });
        power.append(&b);
    }
    let over = gtk4::Overlay::new();
    over.set_child(Some(&col));
    over.add_overlay(&power);
    win.set_child(Some(&over));

    {
        let password = password.clone();
        user.connect_activate(move |_| {
            password.grab_focus();
        });
    }
    let app = app.clone();
    let (u, s) = (user.clone(), session.clone());
    password.connect_activate(move |e| {
        let name = u.text().trim().to_string();
        let Some(chosen) = sessions.get(s.selected() as usize) else {
            error.set_text("No sessions");
            return;
        };
        if name.is_empty() {
            u.grab_focus();
            return;
        }
        let secret = e.text().to_string();
        e.set_text("");
        e.set_sensitive(false);
        error.remove_css_class("error");
        error.add_css_class("dim");
        error.set_text("…");
        let (tx, rx) = async_channel::bounded(1);
        let (who, cmd, env) = (name.clone(), vec![chosen.cmd.clone()], chosen.env.clone());
        std::thread::spawn(move || {
            let r = std::env::var("GREETD_SOCK")
                .map_err(|_| "GREETD_SOCK is not set".to_string())
                .and_then(|p| UnixStream::connect(p).map_err(|e| e.to_string()))
                .and_then(|mut sock| login(&mut sock, &who, &secret, &cmd, &env));
            let _ = tx.send_blocking(r);
        });
        let (e, error, app, session) = (e.clone(), error.clone(), app.clone(), chosen.name.clone());
        glib::spawn_future_local(async move {
            match rx.recv().await.unwrap_or(Err("the login went away".into())) {
                Ok(()) => {
                    let _ = std::fs::write(STATE, format!("{name}\n{session}\n"));
                    app.quit();
                }
                Err(why) => {
                    e.set_sensitive(true);
                    error.remove_css_class("dim");
                    error.add_css_class("error");
                    error.set_text(&why);
                    e.grab_focus();
                }
            }
        });
    });
    let focus = if user.text().is_empty() { user.upcast::<gtk4::Widget>() } else { password.upcast() };
    win.connect_map(move |_| {
        focus.grab_focus();
    });
    win.present();
}

/// The user and the session last logged in with, as far as they are known.
fn remembered() -> (Option<String>, Option<String>) {
    let text = std::fs::read_to_string(STATE).unwrap_or_default();
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from);
    (lines.next(), lines.next())
}

/// The first human in /etc/passwd: a uid from 1000, below nobody's, with a shell that lets one in.
fn first_user() -> Option<String> {
    humans(&std::fs::read_to_string("/etc/passwd").ok()?).into_iter().next()
}

fn humans(passwd: &str) -> Vec<String> {
    passwd
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            let uid: u32 = f.get(2)?.parse().ok()?;
            let shell = f.get(6)?;
            ((1000..65534).contains(&uid) && !shell.ends_with("nologin") && !shell.ends_with("false"))
                .then(|| f[0].to_string())
        })
        .collect()
}

/// A session to start: its name, its command as greetd runs it (through the user's shell), its environment.
struct Session {
    name: String,
    cmd: String,
    env: Vec<String>,
}

/// The Wayland sessions, then the X ones, each directory's files in order.
fn sessions() -> Vec<Session> {
    let mut out = Vec::new();
    for (dir, x) in [("/usr/share/wayland-sessions", false), ("/usr/share/xsessions", true)] {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "desktop"))
            .collect();
        files.sort();
        out.extend(files.iter().filter_map(|f| session(&std::fs::read_to_string(f).ok()?, x)));
    }
    out
}

/// A session of a .desktop file's [Desktop Entry]: its Name, its Exec (an X one through startx), the desktop's
/// names for XDG_CURRENT_DESKTOP; a hidden one none.
fn session(desktop: &str, x: bool) -> Option<Session> {
    let (mut name, mut exec, mut names, mut entry) = (None, None, String::new(), false);
    for line in desktop.lines().map(str::trim) {
        if line.starts_with('[') {
            entry = line == "[Desktop Entry]";
            continue;
        }
        let Some((k, v)) = line.split_once('=').filter(|_| entry) else { continue };
        match (k.trim(), v.trim()) {
            ("Name", v) => name = Some(v.to_string()),
            ("Exec", v) => exec = Some(v.to_string()),
            ("DesktopNames", v) => names = v.trim_end_matches(';').replace(';', ":"),
            ("Hidden" | "NoDisplay", "true") => return None,
            _ => {}
        }
    }
    let (name, exec) = (name?, exec?);
    let cmd = if x { format!("startx /usr/bin/env {exec}") } else { exec };
    let mut env = vec![format!("XDG_SESSION_TYPE={}", if x { "x11" } else { "wayland" })];
    if let Some(first) = names.split(':').next().filter(|n| !n.is_empty()) {
        env.push(format!("XDG_SESSION_DESKTOP={first}"));
        env.push(format!("XDG_CURRENT_DESKTOP={names}"));
    }
    Some(Session { name, cmd, env })
}

/// greetd's answers.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Reply {
    Success,
    Error { error_type: String, description: String },
    AuthMessage { auth_message_type: String, auth_message: String },
}

fn send(w: &mut impl Write, msg: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(msg)?;
    w.write_all(&(body.len() as u32).to_ne_bytes())?;
    w.write_all(&body)
}

fn recv(r: &mut impl Read) -> io::Result<Reply> {
    let mut len = [0; 4];
    r.read_exact(&mut len)?;
    let mut body = vec![0; u32::from_ne_bytes(len) as usize];
    r.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

fn ask(sock: &mut (impl Read + Write), msg: Value) -> Result<Reply, String> {
    send(sock, &msg).and_then(|_| recv(sock)).map_err(|e| e.to_string())
}

/// A login through greetd: the session created for the user, PAM's questions answered with the password, the
/// session started with its command; on any failure the session cancelled, for the next try to create anew.
fn login(
    sock: &mut (impl Read + Write),
    user: &str,
    password: &str,
    cmd: &[String],
    env: &[String],
) -> Result<(), String> {
    let r = converse(sock, user, password, cmd, env);
    if r.is_err() {
        let _ = ask(sock, json!({"type": "cancel_session"}));
    }
    r
}

fn converse(
    sock: &mut (impl Read + Write),
    user: &str,
    password: &str,
    cmd: &[String],
    env: &[String],
) -> Result<(), String> {
    let failed = |error_type: String, description: String| {
        if error_type == "auth_error" { "Wrong password".to_string() } else { description }
    };
    let mut reply = ask(sock, json!({"type": "create_session", "username": user}))?;
    let mut answered = false;
    loop {
        match reply {
            Reply::Success => break,
            Reply::Error { error_type, description } => return Err(failed(error_type, description)),
            Reply::AuthMessage { auth_message_type: t, auth_message: m } => {
                // ponytail: one prompt answered, with the password; a second (a one-time code) refused, its text
                // said: a field per prompt when such a PAM stack comes
                let response = match t.as_str() {
                    "secret" | "visible" if answered => return Err(m),
                    "secret" | "visible" => {
                        answered = true;
                        Some(password)
                    }
                    _ => None,
                };
                reply = ask(sock, json!({"type": "post_auth_message_response", "response": response}))?;
            }
        }
    }
    match ask(sock, json!({"type": "start_session", "cmd": cmd, "env": env}))? {
        Reply::Success => Ok(()),
        Reply::Error { error_type, description } => Err(failed(error_type, description)),
        Reply::AuthMessage { auth_message, .. } => Err(auth_message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// A fake greetd on a socket: each request read checked against the script's, its answer written.
    fn greetd(script: Vec<(Value, Value)>) -> (std::path::PathBuf, std::thread::JoinHandle<()>) {
        let name = format!("ostrov-greet-{}-{:?}", std::process::id(), std::thread::current().id());
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let t = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            for (want, answer) in script {
                let mut len = [0; 4];
                s.read_exact(&mut len).unwrap();
                let mut body = vec![0; u32::from_ne_bytes(len) as usize];
                s.read_exact(&mut body).unwrap();
                assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), want);
                let out = serde_json::to_vec(&answer).unwrap();
                s.write_all(&(out.len() as u32).to_ne_bytes()).unwrap();
                s.write_all(&out).unwrap();
            }
        });
        (path, t)
    }

    fn run(script: Vec<(Value, Value)>) -> Result<(), String> {
        let (path, t) = greetd(script);
        let mut sock = UnixStream::connect(&path).unwrap();
        let r = login(&mut sock, "danil", "pw", &["Hyprland".into()], &["XDG_SESSION_TYPE=wayland".into()]);
        t.join().unwrap();
        let _ = std::fs::remove_file(&path);
        r
    }

    #[test]
    fn logs_in() {
        let r = run(vec![
            (json!({"type": "create_session", "username": "danil"}),
             json!({"type": "auth_message", "auth_message_type": "secret", "auth_message": "Password: "})),
            (json!({"type": "post_auth_message_response", "response": "pw"}), json!({"type": "success"})),
            (json!({"type": "start_session", "cmd": ["Hyprland"], "env": ["XDG_SESSION_TYPE=wayland"]}),
             json!({"type": "success"})),
        ]);
        assert_eq!(r, Ok(()));
    }

    #[test]
    fn wrong_password_cancels() {
        let r = run(vec![
            (json!({"type": "create_session", "username": "danil"}),
             json!({"type": "auth_message", "auth_message_type": "info", "auth_message": "hello"})),
            (json!({"type": "post_auth_message_response", "response": null}),
             json!({"type": "auth_message", "auth_message_type": "secret", "auth_message": "Password: "})),
            (json!({"type": "post_auth_message_response", "response": "pw"}),
             json!({"type": "error", "error_type": "auth_error", "description": "pam_authenticate: AUTH_ERR"})),
            (json!({"type": "cancel_session"}), json!({"type": "success"})),
        ]);
        assert_eq!(r, Err("Wrong password".into()));
    }

    #[test]
    fn parses_sessions_and_users() {
        let s = session("[Desktop Entry]\nName=i3\nExec=i3\nDesktopNames=i3;\n[Action x]\nExec=no\n", true).unwrap();
        assert_eq!((s.name.as_str(), s.cmd.as_str()), ("i3", "startx /usr/bin/env i3"));
        assert_eq!(s.env, ["XDG_SESSION_TYPE=x11", "XDG_SESSION_DESKTOP=i3", "XDG_CURRENT_DESKTOP=i3"]);
        assert!(session("[Desktop Entry]\nName=a\nExec=a\nNoDisplay=true\n", false).is_none());
        let passwd = "root:x:0:0::/root:/bin/bash\n_greetd:x:114:118::/var/lib/greetd:/usr/sbin/nologin\n\
                      nobody:x:65534:65534::/:/usr/sbin/nologin\ndanil:x:1000:1000::/home/danil:/usr/bin/zsh\n";
        assert_eq!(humans(passwd), ["danil"]);
    }
}
