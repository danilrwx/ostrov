//! `ostrov doctor`: what ostrov finds of what it works with, and what to do about what is missing. The compositor
//! (Hyprland and its version, its blur, which of ostrov's keys and the plugins' are bound and which taken by
//! something else),
//! the services on the buses (Wi-Fi's, Bluetooth's, the power profiles', the battery's, the session's, the
//! keyring), the programs it runs, its PAM profile. A line each: ✓ there, ! missing and what that costs, · for
//! information.

use serde_json::Value;

use crate::i18n::{fill, plural, t};
use crate::wm::hyprctl;

type Line = (char, String);

/// The report, its lines made on a thread of their own (the buses asked through a runtime of its own there).
pub async fn report() -> Result<String, String> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
        let _ = tx.send_blocking(rt.block_on(lines()));
    });
    let lines = rx.recv().await.map_err(|e| e.to_string())?;
    Ok(lines.into_iter().map(|(m, t)| format!("{m} {t}")).collect::<Vec<_>>().join("\n"))
}

async fn lines() -> Vec<Line> {
    let mut out = Vec::new();
    compositor(&mut out);
    buses(&mut out).await;
    programs(&mut out);
    let missing = out.iter().filter(|(m, _)| *m == '!').count() as i64;
    if missing > 0 {
        let text = plural(missing, ["{} thing missing", "{} things missing|few", "{} things missing"]);
        out.push(('·', fill(text, &[&missing])));
    }
    out
}

fn compositor(out: &mut Vec<Line>) {
    let v: Value = serde_json::from_str(&hyprctl("j/version")).unwrap_or_default();
    let Some(tag) = v["tag"].as_str() else {
        out.push(('!', t("Hyprland: not running (the window switcher, the overview, the window's title, displays and \
            the night light need it)").into()));
        return;
    };
    out.push(('✓', format!("Hyprland {tag}")));
    let blur: Value = serde_json::from_str(&hyprctl("j/getoption decoration:blur:enabled")).unwrap_or_default();
    if blur["int"].as_i64() == Some(0) {
        out.push(('·', t("blur is off in Hyprland (decoration:blur:enabled): the panels are see-through without it").into()));
    }
    let cfg = crate::config::load().hyprland;
    if !cfg.binds {
        out.push(('·', t("keys: [hyprland] binds = false, ostrov binds none (ostrov hyprland prints them)").into()));
        return;
    }
    let binds: Vec<Value> = serde_json::from_str(&hyprctl("j/binds")).unwrap_or_default();
    for k in crate::modules::hyprland::keys(&binds) {
        let (at, cmd) = (k.at, k.cmd);
        // a plugin's key is where its manifest says: freed, or left unbound
        let plugin = cmd.starts_with("plugin ");
        let fix = t(if plugin { "the plugin's key left unbound" } else { "move it in [hyprland.keys]" });
        match k.by {
            Some(b) if b.contains("ostrov") && b.ends_with(&format!(" {cmd}")) => {
                out.push(('✓', format!("{at}: {cmd}")))
            }
            Some(b) => out.push(('!', fill(t("{} is {}, not ostrov {}: {}"), &[&at, &b, &cmd, &fix]))),
            None => out.push(('·', fill(t("{}: ostrov {} (bound at the next start)"), &[&at, &cmd]))),
        }
    }
}

async fn buses(out: &mut Vec<Line>) {
    let (Ok(system), Ok(session)) = (zbus::Connection::system().await, zbus::Connection::session().await) else {
        out.push(('!', t("D-Bus: no system or session bus").into()));
        return;
    };
    let names = |c: zbus::Connection| async move {
        let Ok(p) = zbus::fdo::DBusProxy::new(&c).await else { return Vec::new() };
        let mut all: Vec<String> = p.list_names().await.unwrap_or_default().into_iter().map(|n| n.to_string()).collect();
        all.extend(p.list_activatable_names().await.unwrap_or_default().into_iter().map(|n| n.to_string()));
        all
    };
    let (sys, ses) = (names(system).await, names(session).await);
    let has = |list: &[String], n: &str| list.iter().any(|x| x == n);
    if has(&sys, "net.connman.iwd") {
        out.push(('✓', "Wi-Fi: iwd".into()));
    } else if has(&sys, "org.freedesktop.NetworkManager") {
        out.push(('✓', "Wi-Fi: NetworkManager".into()));
    } else {
        out.push(('!', t("Wi-Fi: neither iwd nor NetworkManager, the Wi-Fi widget is empty").into()));
    }
    for (name, what, cost) in [
        ("org.bluez", "Bluetooth: BlueZ", "the Bluetooth widget is empty"),
        ("net.hadess.PowerProfiles", "power profiles: power-profiles-daemon", "no Power Mode, no games' profile"),
        ("org.freedesktop.UPower", "battery: UPower", "no battery"),
        ("org.freedesktop.login1", "session: logind", "no brightness, no idle's lock before sleep"),
        ("org.freedesktop.PolicyKit1", "polkit", "no polkit agent"),
    ] {
        let what = t(what);
        out.push(if has(&sys, name) { ('✓', what.into()) } else { ('!', fill(t("{} missing: {}"), &[&what, &t(cost)])) });
    }
    out.push(if has(&ses, "org.freedesktop.secrets") {
        ('✓', t("keyring: the Secret Service").into())
    } else {
        ('·', t("keyring: no Secret Service, secrets kept in ~/.local/share/ostrov/secrets.toml (0600)").into())
    });
}

fn programs(out: &mut Vec<Line>) {
    let on_path = |p: &str| {
        std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|d| d.join(p).is_file()))
    };
    for (p, cost) in [
        ("wpctl", "no volume, no sound devices"),
        ("pw-dump", "no sound devices nor apps playing"),
        ("pw-cli", "an output in another of a card's profiles not switched to"),
        ("gst-launch-1.0", "no screen recording"),
        ("loginctl", "the lock button does nothing"),
    ] {
        out.push(if on_path(p) { ('✓', p.into()) } else { ('!', fill(t("{} missing: {}"), &[&p, &t(cost)])) });
    }
    let limit = crate::modules::battery::limit::state();
    if !limit.is_null() && limit["writable"] != true {
        let text = "charge limit: the thresholds are root's; to let ostrov set them, run: ostrov battery limit install";
        out.push(('·', t(text).into()));
    }
    match crate::greet::stale() {
        Some(true) => out.push(('!', t("login screen: /usr/local/bin/ostrov is another build than this one; \
            ostrov greeter update puts this one there").into())),
        Some(false) => out.push(('✓', t("login screen: this ostrov").into())),
        None => {}
    }
    out.push(if std::path::Path::new("/etc/pam.d/ostrov").exists() {
        ('✓', "PAM: /etc/pam.d/ostrov".into())
    } else {
        ('·', t("PAM: no /etc/pam.d/ostrov, the lock screen checks passwords as login does").into())
    });
}
