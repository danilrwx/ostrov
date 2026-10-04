//! ostrov at home in a Hyprland configured for nothing: what it needs of the compositor put there by itself, over
//! its IPC, at the start and again after every reload of its config (a reload drops what keywords set). Its own
//! layers' rules (the bar and its popups blurred under their see-through ground, the switcher and the overview
//! gone without a fade), the session lock taken over by a new ostrov after one died locked, and its keys, each
//! only where the combination is free: a bind of the user's own is never replaced. [hyprland] turns either off
//! or moves a key; `ostrov hyprland` prints it all as hyprland.conf lines, for those who keep it there instead.

use std::sync::Arc;

use serde_json::Value;
use tokio::io::AsyncBufReadExt;

use super::{Fut, Module};
use crate::services::{Ctx, Kick};
use crate::wm::{hypr_socket, hyprctl};

pub const MODULE: Module = Module { id: "hyprland", worker: Some(worker), ..Module::NONE };

/// Its layers' rules, as layerrule lines.
const RULES: &[&str] = &[
    "blur on, ignore_alpha 0.7, xray on, match:namespace ^ostrov$",
    "blur on, ignore_alpha 0.2, xray on, match:namespace ^(ostrov-toast|ostrov-osd|ostrov-prompt|ostrov-switcher|ostrov-overview)$",
    "no_anim on, match:namespace ^(ostrov-switcher|ostrov-overview)$",
];

/// Its keys: a name ([hyprland.keys] moves one: run = "SUPER, R"), the bind's kind, the combination, the
/// command's words after ostrov.
pub const KEYS: &[(&str, &str, &str, &str)] = &[
    ("run", "bind", "SUPER, D", "run"),
    ("panel", "bind", "SUPER, X", "panel"),
    ("calendar", "bind", "SUPER, C", "calendar"),
    ("windows", "bind", "SUPER, Tab", "windows"),
    ("windows-app", "bind", "SUPER, grave", "windows app"),
    // the switcher's Super let go, once a combination is done: bindrt, fired after other keys too
    ("windows-release", "bindrt", "SUPER, Super_L", "windows release"),
    ("overview", "bind", "SUPER, Up", "overview"),
    ("clip", "bind", "SUPER SHIFT, V", "clip"),
    ("screenshot", "bind", "SUPER SHIFT, S", "screenshot"),
    ("print", "bind", ", Print", "screenshot"),
    ("record", "bind", "SUPER SHIFT, R", "record"),
    ("lock", "bind", "SUPER SHIFT, X", "lock"),
    ("bar", "bind", "SUPER, B", "bar toggle"),
    ("vol-up", "bindel", ", XF86AudioRaiseVolume", "key vol-up"),
    ("vol-down", "bindel", ", XF86AudioLowerVolume", "key vol-down"),
    ("vol-mute", "bindl", ", XF86AudioMute", "key vol-mute"),
    ("mic", "bindl", ", XF86AudioMicMute", "key mic"),
    ("bright-up", "bindel", ", XF86MonBrightnessUp", "key bright-up"),
    ("bright-down", "bindel", ", XF86MonBrightnessDown", "key bright-down"),
    ("play-pause", "bindl", ", XF86AudioPlay", "key play-pause"),
    ("next", "bindl", ", XF86AudioNext", "key next"),
    ("previous", "bindl", ", XF86AudioPrev", "key previous"),
];

/// The gestures it would have, the user's to add (Hyprland lists none to see whether they are free).
const GESTURES: &[&str] = &["3, horizontal, workspace", "3, up, dispatcher, exec, OSTROV overview", "3, down, dispatcher, exec, OSTROV overview close"];

/// The modifiers' mask Hyprland's binds list says, of a combination's "SUPER SHIFT".
fn mask(mods: &str) -> u64 {
    mods.split_whitespace()
        .map(|m| match m.to_uppercase().as_str() {
            "SHIFT" => 1,
            "CAPS" => 2,
            "CTRL" | "CONTROL" => 4,
            "ALT" => 8,
            "MOD2" => 16,
            "MOD3" => 32,
            "SUPER" | "WIN" | "LOGO" | "MOD4" => 64,
            "MOD5" => 128,
            _ => 0,
        })
        .sum()
}

/// The combination a key of its is at, as [hyprland.keys] says or its own.
fn combo(name: &str, own: &str) -> String {
    crate::config::load().hyprland.keys.get(name).cloned().unwrap_or(own.into())
}

/// ostrov's path, for the binds' commands: as it was started, made absolute.
fn me() -> String {
    let arg = std::env::args().next().unwrap_or("ostrov".into());
    if arg.contains('/') {
        std::fs::canonicalize(&arg).map_or(arg, |p| p.to_string_lossy().into_owned())
    } else {
        arg
    }
}

/// What a key would be: its kind, its combination, its command line; and who has its combination now (None
/// free, Some(the command there)).
pub fn keys(binds: &[Value]) -> Vec<(&'static str, String, String, Option<String>)> {
    let me = me();
    KEYS.iter()
        .map(|&(name, kind, own, cmd)| {
            let at = combo(name, own);
            let (mods, key) = at.split_once(',').unwrap_or(("", &at));
            let (m, key) = (mask(mods), key.trim());
            let taken = binds.iter().find(|b| {
                b["modmask"].as_u64() == Some(m)
                    && b["key"].as_str().is_some_and(|k| k.eq_ignore_ascii_case(key))
                    && b["submap"].as_str().unwrap_or("").is_empty()
                    && b["release"].as_bool() == Some(kind.contains('r'))
            });
            let by = taken.map(|b| format!("{} {}", s(b, "dispatcher"), s(b, "arg")).trim().to_string());
            (kind, at.clone(), format!("{at}, exec, {me} {cmd}"), by)
        })
        .collect()
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

/// Its rules, the lock's option, its keys where free: put into the running Hyprland.
fn apply() {
    let cfg = crate::config::load().hyprland;
    let mut batch = vec!["keyword misc:allow_session_lock_restore 1".to_string()];
    if cfg.rules {
        batch.extend(RULES.iter().map(|r| format!("keyword layerrule {r}")));
    }
    if cfg.binds {
        let binds: Vec<Value> = serde_json::from_str(&hyprctl("j/binds")).unwrap_or_default();
        for (kind, _, line, by) in keys(&binds) {
            if by.is_none() {
                batch.push(format!("keyword {kind} {line}"));
            }
        }
    }
    // one request for them all, Hyprland's batch of commands
    hyprctl(&format!("[[BATCH]]{}", batch.join(";")));
}

/// hyprland.conf's lines for what ostrov would put there itself.
pub fn conf() -> String {
    let binds: Vec<Value> = serde_json::from_str(&hyprctl("j/binds")).unwrap_or_default();
    let mut out = vec!["# ostrov (put there by ostrov itself unless [hyprland] says rules or binds = false)".to_string()];
    out.push("misc {\n  allow_session_lock_restore = true\n}".into());
    out.extend(RULES.iter().map(|r| format!("layerrule = {r}")));
    for (kind, _, line, _) in keys(&binds) {
        out.push(format!("{kind} = {line}"));
    }
    out.push("# recommended: gestures (ostrov adds none, Hyprland telling nothing of those there)".into());
    out.extend(GESTURES.iter().map(|g| format!("gesture = {}", g.replace("OSTROV", &me()))));
    out.join("\n")
}

/// At the start, and after every reload of Hyprland's config (its events' configreloaded).
fn worker(_: Arc<Ctx>, _: Kick) -> Fut<'static, ()> {
    Box::pin(async {
        if crate::wm::hypr_socket(".socket.sock").is_none() {
            return;
        }
        tokio::task::spawn_blocking(apply).await.ok();
        let Some(Ok(s)) = hypr_socket(".socket2.sock").map(|p| std::os::unix::net::UnixStream::connect(p)) else { return };
        let _ = s.set_nonblocking(true);
        let Ok(s) = tokio::net::UnixStream::from_std(s) else { return };
        let mut lines = tokio::io::BufReader::new(s).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.starts_with("configreloaded>>") {
                tokio::task::spawn_blocking(apply).await.ok();
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn masks() {
        assert_eq!(mask("SUPER SHIFT"), 65);
        assert_eq!(mask(""), 0);
        assert_eq!(mask("super ctrl alt shift"), 77);
    }

    #[test]
    fn a_taken_combination_is_left_alone() {
        let binds = vec![json!({"modmask": 64, "key": "D", "submap": "", "release": false, "dispatcher": "exec", "arg": "rofi"})];
        let k = keys(&binds);
        let run = k.iter().find(|k| k.1 == "SUPER, D").unwrap();
        assert_eq!(run.3.as_deref(), Some("exec rofi"));
        assert!(k.iter().find(|k| k.1 == "SUPER, X").unwrap().3.is_none());
        // Super's release is another bind than a press of Super
        let release = vec![json!({"modmask": 64, "key": "Super_L", "submap": "", "release": false, "dispatcher": "exec", "arg": "x"})];
        assert!(keys(&release).iter().find(|k| k.1 == "SUPER, Super_L").unwrap().3.is_none());
    }
}
