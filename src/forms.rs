//! Commands' forms, one grammar for ostrov's own, the modules', the blocks' and the plugins': a form is the words
//! after the command's first, space-separated; `a|b|c` alternatives at one place, an UPPERCASE word a placeholder,
//! `[X]` optional, a trailing `...` repeating ("mode off|on|time|sun", "toast TITLE [BODY...]"). From them the
//! usage said, and the shells' completion (`ostrov complete WORD...`), answered by the running ostrov: the words
//! and the placeholders' values from what it knows (the state, the widgets, the settings, the plugins).

use serde_json::Value;

use crate::modules::ALL;

/// A candidate and what it is, a line of `ostrov complete`'s.
pub type Candidates = Vec<(String, String)>;

/// A placeholder's values, given the words before it.
type Resolve<'a> = dyn Fn(&[&str], &str) -> Candidates + 'a;

/// One place of a form.
#[derive(Debug, PartialEq)]
struct Slot<'a> {
    alts: Vec<&'a str>,
    optional: bool,
    repeat: bool,
}

impl Slot<'_> {
    fn matches(&self, word: &str) -> bool {
        self.alts.iter().any(|a| placeholder(a) || *a == word)
    }
}

fn placeholder(word: &str) -> bool {
    word.chars().any(|c| c.is_ascii_uppercase()) && !word.chars().any(|c| c.is_ascii_lowercase())
}

fn parse(form: &str) -> Vec<Slot<'_>> {
    form.split_whitespace()
        .map(|w| {
            let (w, repeat) = w.strip_suffix("...").map_or((w, false), |w| (w, true));
            let (w, optional) = w.strip_prefix('[').and_then(|w| w.strip_suffix(']')).map_or((w, false), |w| (w, true));
            let (w, repeat) = w.strip_suffix("...").map_or((w, repeat), |w| (w, true));
            Slot { alts: w.split('|').collect(), optional, repeat }
        })
        .collect()
}

/// "usage: ostrov ID FORM | FORM ...", what a command's wrong words get.
pub fn usage(id: &str, forms: &[&str]) -> String {
    let all = forms.join(" | ");
    format!("usage: ostrov {}", [id, &all].join(" ").trim())
}

/// The slots a form may be at once words are given; none when they do not fit it.
fn reach<'s, 'a>(slots: &'s [Slot<'a>], words: &[&str], out: &mut Vec<&'s Slot<'a>>) {
    let Some((s, rest)) = slots.split_first() else { return };
    match words.split_first() {
        None => out.push(s),
        Some((w, more)) if s.matches(w) => reach(if s.repeat { slots } else { rest }, more, out),
        Some(_) => {}
    }
    if s.optional {
        reach(rest, words, out);
    }
}

/// What may come after words in one of forms (each with what it is, said by its first word), starting with
/// partial: a form's words as they are, a placeholder's values from resolve (given the words before it).
fn candidates(forms: &[(&str, &str)], words: &[&str], partial: &str, resolve: &Resolve) -> Candidates {
    let mut out: Candidates = Vec::new();
    for (form, about) in forms {
        let slots = parse(form);
        let mut next = Vec::new();
        reach(&slots, words, &mut next);
        for s in next {
            let first = std::ptr::eq(s, &slots[0]);
            for a in &s.alts {
                if placeholder(a) {
                    out.extend(resolve(words, a));
                } else {
                    out.push((a.to_string(), if first { about.to_string() } else { String::new() }));
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|(c, _)| c.starts_with(partial) && seen.insert(c.clone()));
    out
}

/// A plugin as completion knows it: its id, its name, its commands' forms with their help.
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub commands: Vec<(String, String)>,
}

/// What the running ostrov knows, for the placeholders: the state, the control centre's widgets (and the menus'
/// other names), the Settings' entries, the plugins, the bar's blocks with commands, the themes.
pub struct Known {
    pub state: Value,
    pub widgets: Candidates,
    pub sections: Candidates,
    pub plugins: Vec<Plugin>,
    pub blocks: Vec<(String, &'static [&'static str])>,
    pub themes: Candidates,
}

/// `ostrov complete WORD...`: what may follow the words after `ostrov`, the last one as far as it is typed.
pub fn complete(k: &Known, words: &[&str]) -> Candidates {
    let (partial, done) = words.split_last().unwrap_or((&"", &[]));
    let own = |w: &[&str], ph: &str| -> Candidates {
        match (w.first().copied(), ph) {
            (Some("menu"), "NAME") => k.widgets.clone(),
            (Some("settings"), "SECTION") => k.sections.clone(),
            (Some("key"), "NAME") => crate::keys::NAMES.iter().map(|n| (n.to_string(), String::new())).collect(),
            (Some("theme"), "ID") => k.themes.clone(),
            (Some("plugin"), "ID") => k.plugins.iter().map(|p| (p.id.clone(), p.name.clone())).collect(),
            _ => Vec::new(),
        }
    };
    let pair = |f: &&'static str| (*f, *f);
    match done {
        [] => {
            let mut top: Vec<(String, String)> = crate::FORMS.iter().map(|f| (f.to_string(), f.to_string())).collect();
            let modules = ALL.iter().filter(|m| m.run.is_some()).map(|m| (m.id, m.forms));
            for (id, forms) in modules.chain(k.blocks.iter().map(|(b, f)| (b.as_str(), *f))) {
                top.push((id.to_string(), forms.join(" | ")));
            }
            let top: Vec<(&str, &str)> = top.iter().map(|(f, a)| (f.as_str(), a.as_str())).collect();
            candidates(&top, &[], partial, &own)
        }
        [first, rest @ ..] => {
            if let Some(m) = ALL.iter().find(|m| m.id == *first && m.run.is_some()) {
                let forms: Vec<_> = m.forms.iter().map(pair).collect();
                let resolve = |_: &[&str], ph: &str| m.complete.map_or(Vec::new(), |f| f(&k.state[m.id], ph));
                return candidates(&forms, rest, partial, &resolve);
            }
            if let Some((_, forms)) = k.blocks.iter().find(|(b, _)| b == first) {
                let forms: Vec<_> = forms.iter().map(pair).collect();
                return candidates(&forms, rest, partial, &own);
            }
            // a plugin's own commands; `plugin install|remove` ostrov's
            if let ("plugin", [id, rest @ ..]) = (*first, rest)
                && let Some(p) = k.plugins.iter().find(|p| p.id == *id)
            {
                let forms: Vec<_> = p.commands.iter().map(|(f, h)| (f.as_str(), h.as_str())).collect();
                return candidates(&forms, rest, partial, &|_, _| Vec::new());
            }
            let forms: Vec<_> = crate::FORMS.iter().map(pair).collect();
            candidates(&forms, done, partial, &own)
        }
    }
}

/// The candidates as `ostrov complete` prints them: a line each, the candidate and what it is, a tab between.
pub fn lines(c: &Candidates) -> String {
    c.iter().map(|(w, about)| format!("{w}\t{about}")).collect::<Vec<_>>().join("\n")
}

/// `ostrov completions SHELL`: the script asking the running ostrov, also in completions/ for packaging.
pub fn script(shell: &str) -> Option<&'static str> {
    match shell {
        "zsh" => Some(include_str!("../completions/_ostrov")),
        "bash" => Some(include_str!("../completions/ostrov.bash")),
        "fish" => Some(include_str!("../completions/ostrov.fish")),
        _ => None,
    }
}

/// Whether an ostrov runs as id on the session bus: `ostrov complete` with none is answered by nothing, never by
/// starting one (GApplication would make the asking one the shell).
pub fn running(id: &str) -> bool {
    use gtk4::gio;
    use gtk4::prelude::*;
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else { return false };
    let r = bus.call_sync(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        Some(&(id,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        1000,
        None::<&gio::Cancellable>,
    );
    r.ok().and_then(|v| v.get::<(bool,)>()).is_some_and(|(b,)| b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn words(c: &Candidates) -> Vec<&str> {
        c.iter().map(|(w, _)| w.as_str()).collect()
    }

    #[test]
    fn grammar() {
        let s = parse("toast TITLE [BODY...]");
        assert_eq!(s[0], Slot { alts: vec!["toast"], optional: false, repeat: false });
        assert_eq!(s[2], Slot { alts: vec!["BODY"], optional: true, repeat: true });
        assert_eq!(parse("[ARGS]...")[0], Slot { alts: vec!["ARGS"], optional: true, repeat: true });
        assert_eq!(parse("mode off|on|time|sun")[1].alts, ["off", "on", "time", "sun"]);
        assert!(placeholder("SSID") && placeholder("K") && !placeholder("--audio") && !placeholder("on"));
        assert_eq!(usage("wifi", &["on|off", "connect SSID"]), "usage: ostrov wifi on|off | connect SSID");
        assert_eq!(usage("headset", &[""]), "usage: ostrov headset");
    }

    #[test]
    fn forms_walked() {
        let none = |_: &[&str], _: &str| Vec::new();
        let ph = |w: &[&str], p: &str| vec![(format!("<{p}:{}>", w.join(" ")), String::new())];
        let night = [("mode off|on|time|sun", "m"), ("time FROM TO", "t"), ("apply", "a")];
        assert_eq!(words(&candidates(&night, &[], "", &none)), ["mode", "time", "apply"]);
        assert_eq!(candidates(&night, &[], "", &none)[0].1, "m");
        assert_eq!(words(&candidates(&night, &[], "ti", &none)), ["time"]);
        assert_eq!(words(&candidates(&night, &["mode"], "", &none)), ["off", "on", "time", "sun"]);
        assert_eq!(candidates(&night, &["mode"], "o", &none)[0].1, "");
        assert_eq!(words(&candidates(&night, &["time"], "", &ph)), ["<FROM:time>"]);
        assert_eq!(words(&candidates(&night, &["time", "1"], "", &ph)), ["<TO:time 1>"]);
        assert!(candidates(&night, &["time", "1", "2"], "", &ph).is_empty());
        assert!(candidates(&night, &["nope"], "", &ph).is_empty());
        let opt = [("windows [app]", ""), ("record [--audio]", ""), ("toast TITLE [BODY...]", "")];
        assert_eq!(words(&candidates(&opt, &["windows"], "", &none)), ["app"]);
        assert_eq!(words(&candidates(&opt, &["record"], "--", &none)), ["--audio"]);
        assert_eq!(words(&candidates(&opt, &["toast", "t", "a", "b"], "", &ph)), ["<BODY:toast t a b>"]);
        let opt2 = [("settings [SECTION] [X]", "")];
        assert_eq!(words(&candidates(&opt2, &["settings"], "", &ph)), ["<SECTION:settings>", "<X:settings>"]);
    }

    fn known() -> Known {
        Known {
            state: json!({
                "wifi": {"networks": [
                    {"ssid": "Home", "signal": 4, "security": "psk", "known": true, "connected": true},
                    {"ssid": "Cafe", "signal": 2, "security": "open", "known": false, "connected": false},
                ]},
                "bt": {"devices": [{"address": "AA:BB", "name": "Buds", "connected": true}]},
                "power": {"profiles": ["power-saver", "balanced", "performance"]},
                "displays": {"monitors": [{"name": "eDP-1", "description": "Panel"}], "profiles": ["desk"]},
                "audio": {"streams": [{"id": 116, "name": "Chromium"}]},
            }),
            widgets: vec![("wifi".into(), "Wi-Fi".into()), ("ins".into(), "mic".into())],
            sections: vec![("bar".into(), "Bar".into())],
            plugins: vec![Plugin {
                id: "hello".into(),
                name: "Hello".into(),
                commands: vec![("set N".into(), "the count set".into()), ("mode a|b".into(), String::new())],
            }],
            blocks: vec![("status".into(), &["menu NAME", "settings [SECTION]", "appearance"])],
            themes: vec![("nord".into(), "Nord".into()), ("paper".into(), "Paper".into())],
        }
    }

    #[test]
    fn completed() {
        let k = known();
        let c = |w: &[&str]| complete(&k, w);
        let top = c(&[""]);
        assert!(["panel", "wifi", "status", "plugin", "complete"].iter().all(|w| words(&top).contains(w)), "{top:?}");
        assert_eq!(words(&c(&["wi"])), ["wifi"]);
        assert!(top.iter().any(|(w, a)| w == "wifi" && a.contains("connect SSID")));
        assert_eq!(words(&c(&["wifi", ""])), ["on", "off", "scan", "disconnect", "connect", "forget"]);
        assert_eq!(c(&["wifi", "connect", ""]), [
            ("Home".into(), "psk, 4/4, known, connected".into()),
            ("Cafe".into(), "open, 2/4".into())
        ]);
        assert_eq!(c(&["bt", "connect", "A"]), [("AA:BB".into(), "Buds".into())]);
        assert_eq!(words(&c(&["power", "set", "b"])), ["balanced"]);
        assert_eq!(words(&c(&["displays", "on", ""])), ["eDP-1"]);
        assert_eq!(words(&c(&["displays", "load", ""])), ["desk"]);
        assert_eq!(c(&["audio", "volume", ""]), [("116".into(), "Chromium".into())]);
        assert_eq!(words(&c(&["menu", ""])), ["wifi", "ins"]);
        assert_eq!(words(&c(&["status", "menu", "w"])), ["wifi"]);
        assert_eq!(words(&c(&["settings", ""])), ["bar"]);
        assert_eq!(words(&c(&["key", "vol-m"])), ["vol-mute"]);
        assert_eq!(words(&c(&["plugin", ""])), ["hello", "install", "remove"]);
        assert_eq!(c(&["plugin", "remove", ""]), [("hello".into(), "Hello".into())]);
        assert!(c(&["plugin", "install", ""]).is_empty());
        assert_eq!(c(&["plugin", "hello", ""]), [
            ("set".into(), "the count set".into()),
            ("mode".into(), String::new())
        ]);
        assert_eq!(words(&c(&["plugin", "hello", "mode", ""])), ["a", "b"]);
        assert!(c(&["plugin", "nope", ""]).is_empty());
        assert_eq!(words(&c(&["completions", ""])), ["zsh", "bash", "fish"]);
        assert_eq!(words(&c(&["theme", ""])), ["list", "set", "install", "remove"]);
        assert_eq!(c(&["theme", "set", "p"]), [("paper".into(), "Paper".into())]);
        assert_eq!(words(&c(&["theme", "remove", ""])), ["nord", "paper"]);
        assert!(c(&["nope", ""]).is_empty());
        assert_eq!(lines(&c(&["power", "set", "p"])), "power-saver\t\nperformance\t");
    }
}
