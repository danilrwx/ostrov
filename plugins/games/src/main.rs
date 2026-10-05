//! Games played in a power profile of their own, gamemode's work without it: while a game's window has the focus
//! (its class one of [plugin.games] classes, "steam_app_*" any of Steam's), the power profile is the one games want,
//! the one before put back as the focus leaves it. Only a profile set here is put back: one picked by hand before
//! the game stays. The profile is read and set with powerprofilesctl, on a thread of its own: a call answers in 10 s,
//! and the window events come in order there.

use std::process::Command;
use std::sync::mpsc::{Receiver, Sender, channel};

use ostrov_plugin::{Host, Plugin, Value, json, t};

/// A window focused: its class, the game classes and the profile as the config says now.
type Focus = (String, Vec<String>, String);

struct Games {
    tx: Sender<Focus>,
}

/// A window's class one of the games: equal, or a prefix before a trailing *.
fn is_game(class: &str, classes: &[String]) -> bool {
    classes.iter().any(|c| match c.strip_suffix('*') {
        Some(prefix) => class.starts_with(prefix),
        None => class == c,
    })
}

/// powerprofilesctl ARGS: its output, or what went wrong.
fn ppctl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("powerprofilesctl").args(args).output().map_err(|e| format!("powerprofilesctl: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The profile before the game, while a game has the focus.
fn follow(rx: Receiver<Focus>) {
    let mut before: Option<String> = None;
    for (class, classes, profile) in rx {
        let game = is_game(&class, &classes);
        if game && before.is_none() {
            let now = ppctl(&["get"]).unwrap_or_default();
            if now != profile && !now.is_empty() {
                if let Err(e) = ppctl(&["set", &profile]) {
                    eprintln!("games: {e}");
                    continue;
                }
                before = Some(now);
            }
        } else if !game
            && let Some(p) = before.take()
                && let Err(e) = ppctl(&["set", &p]) {
                    eprintln!("games: {e}");
                }
    }
}

impl Plugin for Games {
    fn on_config(&mut self, host: &Host, _: &Value) {
        host.set_settings_schema(&json!({"sections": [{"title": t("Games"), "fields": [
            {"key": "classes", "title": t("Window classes"), "type": "list",
             "help": t("Their windows focused, the power profile below; a trailing * a prefix: dota2, cs2, steam_app_*.")},
            {"key": "profile", "title": t("Power profile"), "type": "choice", "default": "performance", "options": [
                {"value": "performance", "label": t("Performance")},
                {"value": "balanced", "label": t("Balanced")},
                {"value": "power-saver", "label": t("Power Saver")},
            ]},
        ]}]}));
    }

    fn on_shell_event(&mut self, host: &Host, name: &str, payload: &Value) {
        if name == "window" {
            let class = payload["class"].as_str().unwrap_or("").to_string();
            let classes = host.setting("classes").unwrap_or_default();
            let profile = host.setting("profile").unwrap_or_else(|| "performance".to_string());
            let _ = self.tx.send((class, classes, profile));
        }
    }
}

fn main() {
    let (tx, rx) = channel();
    std::thread::spawn(move || follow(rx));
    ostrov_plugin::run(Games { tx });
}

#[cfg(test)]
mod tests {
    #[test]
    fn is_game() {
        let classes = ["dota2".to_string(), "steam_app_*".to_string()];
        assert!(super::is_game("dota2", &classes));
        assert!(super::is_game("steam_app_570", &classes));
        assert!(!super::is_game("dota", &classes));
        assert!(!super::is_game("Alacritty", &classes));
    }
}
