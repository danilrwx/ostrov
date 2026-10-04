//! ostrov at home in a Hyprland configured for nothing: what it needs of the compositor put there by itself, over
//! its IPC, at the start and again after every reload of its config (a reload drops what keywords set). Its own
//! layers' rules (the bar and its popups blurred under their see-through ground, the switcher and the overview
//! gone without a fade), the session lock taken over by a new ostrov after one died locked, and its keys, each
//! only where the combination is free: a bind of the user's own is never replaced; the plugins' keys ([[keys]])
//! the same way, after ostrov's own. [hyprland] turns either off
//! or moves a key; `ostrov hyprland` prints it all as hyprland.conf lines, for those who keep it there instead.

use std::sync::{Arc, Mutex, MutexGuard};

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
    // a laptop's vendor keys: its settings key, its notification centre's, the projection one
    ("tools", "bind", ", XF86Tools", "settings"),
    ("notifications", "bind", ", XF86NotificationCenter", "calendar"),
    ("display", "bind", ", XF86Display", "menu displays"),
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

/// A key as it would be bound: its bind's kind, its combination, the command's words after ostrov, its bind's
/// line; and who has its combination now (None free, Some(the command there)).
pub struct Key {
    pub kind: &'static str,
    pub at: String,
    pub cmd: String,
    pub line: String,
    pub by: Option<String>,
}

/// A plugin's key ([[keys]] of one with "keys"): the plugin's id, the combination, its command's words.
pub type PluginKey = (String, String, String);

/// The plugins' keys, set by plugins/ as they are loaded and removed.
static PLUGIN_KEYS: Mutex<Vec<PluginKey>> = Mutex::new(Vec::new());

/// One apply at a time: two at once would both find a combination free and bind it twice.
static APPLYING: Mutex<()> = Mutex::new(());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A combination's modifiers' mask and its key, as Hyprland's binds list says them ("SUPER, D": 64, "d").
fn split(at: &str) -> (u64, String) {
    let (mods, key) = at.split_once(',').unwrap_or(("", at));
    (mask(mods), key.trim().to_lowercase())
}

/// Whether a bind of Hyprland's list is at that combination, outside submaps.
fn at(b: &Value, combo: &(u64, String)) -> bool {
    b["modmask"].as_u64() == Some(combo.0)
        && b["key"].as_str().is_some_and(|k| k.eq_ignore_ascii_case(&combo.1))
        && b["submap"].as_str().unwrap_or("").is_empty()
}

/// ostrov's keys, then the plugins' (`ostrov plugin ID WORDS`, plain binds).
pub fn keys(binds: &[Value]) -> Vec<Key> {
    let own = KEYS.iter().map(|&(name, kind, own, cmd)| (kind, combo(name, own), cmd.to_string()));
    let plugins = lock(&PLUGIN_KEYS).clone().into_iter();
    let plugins = plugins.map(|(id, at, words)| ("bind", at, format!("plugin {id} {words}")));
    claim(binds, own.chain(plugins), &me())
}

/// Each key wanted (its kind, combination, words) with who has its combination: a bind of Hyprland's there, else
/// a free key before it in the list (bound in the same go, so the first wanting a combination has it).
fn claim(binds: &[Value], wanted: impl Iterator<Item = (&'static str, String, String)>, me: &str) -> Vec<Key> {
    let mut out: Vec<Key> = Vec::new();
    for (kind, combo, cmd) in wanted {
        let (keys, release) = (split(&combo), kind.contains('r'));
        let taken = binds.iter().find(|b| at(b, &keys) && b["release"].as_bool() == Some(release));
        let by = taken.map(|b| format!("{} {}", s(b, "dispatcher"), s(b, "arg")).trim().to_string());
        let before = |k: &&Key| k.by.is_none() && k.kind.contains('r') == release && split(&k.at) == keys;
        let by = by.or_else(|| out.iter().find(before).map(|k| format!("exec {me} {}", k.cmd)));
        out.push(Key { kind, line: format!("{combo}, exec, {me} {cmd}"), at: combo, cmd, by });
    }
    out
}

/// The plugins' keys set anew and bound where free, those of plugins gone (dropped) unbound first where they are
/// still theirs; on a thread of its own, Hyprland's IPC blocking.
pub fn plugin_keys(all: Vec<PluginKey>, dropped: Vec<PluginKey>) {
    *lock(&PLUGIN_KEYS) = all;
    if hypr_socket(".socket.sock").is_none() {
        return;
    }
    std::thread::spawn(move || {
        let binds: Vec<Value> = serde_json::from_str(&hyprctl("j/binds")).unwrap_or_default();
        let me = me();
        for (id, combo, words) in dropped {
            let theirs = format!("{me} plugin {id} {words}");
            if binds.iter().any(|b| at(b, &split(&combo)) && s(b, "arg") == theirs) {
                hyprctl(&format!("keyword unbind {combo}"));
            }
        }
        apply();
    });
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

/// Its rules, the lock's option, its keys where free: put into the running Hyprland.
fn apply() {
    let cfg = crate::config::load().hyprland;
    let _one = lock(&APPLYING);
    let mut batch = vec!["keyword misc:allow_session_lock_restore 1".to_string()];
    if cfg.rules {
        batch.extend(RULES.iter().map(|r| format!("keyword layerrule {r}")));
    }
    if cfg.binds {
        let binds: Vec<Value> = serde_json::from_str(&hyprctl("j/binds")).unwrap_or_default();
        for k in keys(&binds).into_iter().filter(|k| k.by.is_none()) {
            batch.push(format!("keyword {} {}", k.kind, k.line));
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
    for k in keys(&binds) {
        out.push(format!("{} = {}", k.kind, k.line));
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
        let run = k.iter().find(|k| k.at == "SUPER, D").unwrap();
        assert_eq!(run.by.as_deref(), Some("exec rofi"));
        assert!(k.iter().find(|k| k.at == "SUPER, X").unwrap().by.is_none());
        // Super's release is another bind than a press of Super
        let release = vec![json!({"modmask": 64, "key": "Super_L", "submap": "", "release": false, "dispatcher": "exec", "arg": "x"})];
        assert!(keys(&release).iter().find(|k| k.at == "SUPER, Super_L").unwrap().by.is_none());
    }

    #[test]
    fn a_plugins_key_only_where_free() {
        let binds =
            vec![json!({"modmask": 65, "key": "F11", "submap": "", "release": false, "dispatcher": "exec", "arg": "foo"})];
        let wanted = [
            ("bind", "SUPER, D".to_string(), "run".to_string()),
            ("bindrt", "SUPER, Super_L".to_string(), "windows release".to_string()),
            ("bind", "SUPER, F12".to_string(), "plugin a toggle".to_string()),
            ("bind", "super, d".to_string(), "plugin a run".to_string()),
            ("bind", "SUPER SHIFT, f11".to_string(), "plugin a other".to_string()),
            ("bind", "SUPER, F12".to_string(), "plugin b toggle".to_string()),
            ("bind", "SUPER, Super_L".to_string(), "plugin b press".to_string()),
        ];
        let k = claim(&binds, wanted.into_iter(), "/bin/ostrov");
        let by: Vec<Option<&str>> = k.iter().map(|k| k.by.as_deref()).collect();
        assert_eq!(by, [
            None,
            None,
            None,
            Some("exec /bin/ostrov run"),
            Some("exec foo"),
            Some("exec /bin/ostrov plugin a toggle"),
            // a press is another bind than the release before it
            None,
        ]);
        assert_eq!(k[2].line, "SUPER, F12, exec, /bin/ostrov plugin a toggle");
        assert_eq!(k[2].cmd, "plugin a toggle");
    }
}
