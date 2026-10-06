//! ostrov's displays, as Windows' Win+P: the laptop's screen only, the external ones only, both side by side
//! (extend, the external ones on the side `side` says) or the same picture on all (mirror), each a few monitor rules
//! set with `hyprctl keyword monitor`, the monitors read from `hyprctl -j monitors all`. Layouts kept by name and
//! applied as monitors are plugged in are kanshi's work: while kanshi runs, its profiles (~/.config/kanshi/config)
//! are listed under the modes and switched to with `kanshictl switch`. The rules last until Hyprland reloads its
//! config, as any `hyprctl keyword` does. Hyprland's event socket is followed on a thread of its own, so the widget
//! is drawn again as a monitor is plugged in or out.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;

use ostrov_plugin::{Host, Plugin, Value, fill, json, t};

const MODES: [&str; 4] = ["laptop", "external", "extend", "mirror"];
/// Where the external screens go when extending, as the setting says it and as Hyprland's position does.
const SIDES: [(&str, &str); 4] =
    [("right", "auto-right"), ("left", "auto-left"), ("above", "auto-up"), ("below", "auto-down")];

/// A monitor as Hyprland tells it.
#[derive(Clone, Debug, PartialEq)]
struct Mon {
    name: String,
    enabled: bool,
    /// the name of the monitor this one mirrors, "" for none
    mirror: String,
    mode: String,
    scale: f64,
}

/// `hyprctl -j monitors all`'s answer read, the disabled monitors with the rest.
fn parse(text: &str) -> Vec<Mon> {
    let list: Value = ostrov_plugin::serde_json::from_str(text).unwrap_or_default();
    let mon = |v: &Value| Mon {
        name: v["name"].as_str().unwrap_or("").into(),
        enabled: !v["disabled"].as_bool().unwrap_or(false),
        mirror: v["mirrorOf"].as_str().filter(|m| *m != "none").unwrap_or("").into(),
        mode: format!("{}x{}@{:.2}", v["width"], v["height"], v["refreshRate"].as_f64().unwrap_or(0.0)),
        scale: v["scale"].as_f64().unwrap_or(1.0),
    };
    list.as_array().into_iter().flatten().map(mon).filter(|m| !m.name.is_empty()).collect()
}

fn hyprctl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("hyprctl").args(args).output().map_err(|e| format!("hyprctl: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn monitors() -> Vec<Mon> {
    hyprctl(&["-j", "monitors", "all"]).map(|s| parse(&s)).unwrap_or_default()
}

/// The laptop's own screen (eDP, LVDS, DSI), else the first one, and the others, the external ones.
fn split(mons: &[Mon]) -> Option<(&Mon, Vec<&Mon>)> {
    let builtin = |m: &&Mon| ["eDP", "LVDS", "DSI"].iter().any(|p| m.name.starts_with(p));
    let main = mons.iter().find(builtin).or(mons.first())?;
    Some((main, mons.iter().filter(|m| m.name != main.name).collect()))
}

/// The mode the monitors are in, None with a single screen.
fn current(mons: &[Mon]) -> Option<&'static str> {
    let (main, ext) = split(mons)?;
    let on: Vec<&&Mon> = ext.iter().filter(|m| m.enabled).collect();
    Some(match () {
        _ if ext.is_empty() => return None,
        _ if !main.enabled => "external",
        _ if on.is_empty() => "laptop",
        _ if on.iter().all(|m| m.mirror == main.name) => "mirror",
        _ => "extend",
    })
}

/// The rule turning m on at pos: its mode and scale kept while it is on and shows its own picture, else Hyprland's
/// preferred mode and its own scale.
fn on(m: &Mon, pos: &str) -> String {
    if m.enabled && m.mirror.is_empty() {
        format!("{},{},{pos},{}", m.name, m.mode, m.scale)
    } else {
        format!("{},preferred,{pos},auto", m.name)
    }
}

/// A mode's monitor rules, the screens turned on before the others are turned off, so none is left without one
/// on midway.
fn rules(mode: &str, mons: &[Mon], side: &str) -> Result<Vec<String>, String> {
    let (main, ext) = split(mons).ok_or(t("no monitors: not under Hyprland"))?;
    if ext.is_empty() {
        return Err(t("a single screen").into());
    }
    let off = |m: &&Mon| format!("{},disable", m.name);
    let pos = SIDES.iter().find(|(s, _)| *s == side).map_or("auto-right", |(_, p)| p);
    let rules = match mode {
        "laptop" => [on(main, "0x0")].into_iter().chain(ext.iter().map(off)).collect(),
        "external" => {
            let ons = ext.iter().enumerate().map(|(i, m)| on(m, if i == 0 { "0x0" } else { "auto-right" }));
            ons.chain([off(&main)]).collect()
        }
        "extend" => [on(main, "0x0")].into_iter().chain(ext.iter().map(|m| on(m, pos))).collect(),
        "mirror" => {
            let mirror = |m: &&Mon| format!("{},preferred,auto,auto,mirror,{}", m.name, main.name);
            [on(main, "0x0")].into_iter().chain(ext.iter().map(mirror)).collect()
        }
        _ => return Err(format!("no mode {mode}: {}", MODES.join(", "))),
    };
    Ok(rules)
}

/// The rules set one by one; Hyprland answers each with "ok" or what is wrong with it.
fn apply(rules: &[String]) -> Result<(), String> {
    for r in rules {
        match hyprctl(&["keyword", "monitor", r])?.as_str() {
            "ok" => {}
            e => return Err(format!("{r}: {e}")),
        }
    }
    Ok(())
}

/// The profiles kanshi's config names, `profile NAME {` lines; an unnamed profile has no name to switch to.
fn kanshi_profiles(config: &str) -> Vec<String> {
    let names = config.lines().filter_map(|l| {
        let rest = l.trim().strip_prefix("profile")?;
        let name = rest.split('{').next()?.trim().trim_matches('"');
        (rest.starts_with(char::is_whitespace) && !name.is_empty()).then(|| name.to_string())
    });
    names.collect()
}

/// kanshi's profiles, while it runs.
fn kanshi() -> Vec<String> {
    let running = Command::new("pgrep").args(["-x", "kanshi"]).output().is_ok_and(|o| o.status.success());
    if !running {
        return vec![];
    }
    let dir = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let dir = dir.or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    let text = dir.and_then(|d| std::fs::read_to_string(d.join("kanshi/config")).ok());
    kanshi_profiles(&text.unwrap_or_default())
}

fn switch(profile: &str) -> Result<(), String> {
    let out = Command::new("kanshictl").args(["switch", profile]).output().map_err(|e| format!("kanshictl: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// A plugged monitor or one unplugged, from Hyprland's event socket: the widget drawn again.
fn watch(host: Host) {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    let Ok(sig) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE") else { return };
    let Ok(sock) = UnixStream::connect(format!("{dir}/hypr/{sig}/.socket2.sock")) else { return };
    for line in BufReader::new(sock).lines() {
        let Ok(line) = line else { return };
        if line.starts_with("monitoradded") || line.starts_with("monitorremoved") {
            host.kick();
        }
    }
}

/// What went wrong said in a toast, from a thread: a toast is one of ostrov's commands, answered later.
fn say(host: &Host, e: String) {
    let h = host.clone();
    std::thread::spawn(move || {
        let _ = h.toast(t("Displays"), &e);
    });
}

fn label(mode: &str) -> (&'static str, &'static str) {
    match mode {
        "laptop" => ("computer-symbolic", t("Laptop screen only")),
        "external" => ("video-display-symbolic", t("External screen only")),
        "extend" => ("view-dual-symbolic", t("Extend")),
        _ => ("view-mirror-symbolic", t("Mirror")),
    }
}

struct Displays {
    side: String,
    watching: bool,
}

impl Displays {
    fn set(&self, mode: &str) -> Result<(), String> {
        apply(&rules(mode, &monitors(), &self.side)?)
    }
}

impl Plugin for Displays {
    fn on_config(&mut self, host: &Host, _: &Value) {
        let sides: Vec<Value> = [t("right"), t("left"), t("above"), t("below")]
            .iter()
            .zip(SIDES)
            .map(|(label, (value, _))| json!({"value": value, "label": label}))
            .collect();
        host.set_settings_schema(&json!({"sections": [{"title": t("Displays"), "fields": [
            {"key": "side", "title": t("Extend to the"), "type": "choice", "options": sides, "default": "right",
             "help": t("where the external screen goes, beside the laptop's")},
        ]}]}));
        self.side = host.setting("side").unwrap_or_else(|| "right".into());
        if !self.watching {
            self.watching = true;
            let h = host.clone();
            std::thread::spawn(move || watch(h));
        }
        host.kick();
    }

    fn state(&mut self, _: &Host) -> Value {
        let mons = monitors();
        let names: Vec<&str> = mons.iter().map(|m| m.name.as_str()).collect();
        json!({"monitors": names, "mode": current(&mons), "kanshi": kanshi()})
    }

    fn run(&mut self, host: &Host, args: &[&str], _: Option<&str>) -> Result<String, String> {
        match args {
            // ostrov's own command, answered once this one is: asked from a thread
            ["menu"] => {
                let h = host.clone();
                std::thread::spawn(move || h.run(&["menu", "plugin.displays.displays"]));
            }
            ["mode", mode] => self.set(mode)?,
            ["profile", name] => switch(name)?,
            _ => return Err(format!("no command {:?}", args.join(" "))),
        }
        host.kick();
        Ok(String::new())
    }

    fn render(&mut self, _: &Host, widget: &str) -> Option<Value> {
        if widget != "displays" {
            return None;
        }
        let mons = monitors();
        let on = mons.iter().filter(|m| m.enabled).count();
        let cur = current(&mons);
        let sub = match cur {
            Some(m) => label(m).1.to_string(),
            None if mons.is_empty() => t("no monitors").to_string(),
            None => fill(t("{} on"), &[&on]),
        };
        let mut items = vec![];
        if let (Some((main, ext)), Some(_)) = (split(&mons), cur) {
            let ext: Vec<&str> = ext.iter().map(|m| m.name.as_str()).collect();
            for mode in MODES {
                let (icon, text) = label(mode);
                let note = match mode {
                    "laptop" => main.name.clone(),
                    "external" => ext.join(", "),
                    _ => String::new(),
                };
                items.push(json!({"type": "row", "id": format!("mode:{mode}"), "icon": icon, "text": text,
                    "note": note, "on": cur == Some(mode)}));
            }
        } else {
            items.push(json!({"type": "label", "class": "dim", "text": t("A single screen")}));
        }
        let profiles = kanshi();
        if !profiles.is_empty() {
            items.push(json!({"type": "separator"}));
            items.push(json!({"type": "label", "class": "dim", "text": t("kanshi's profiles")}));
            for p in profiles {
                items.push(json!({"type": "row", "id": format!("profile:{p}"), "icon": "view-list-symbolic",
                    "text": p}));
            }
        }
        Some(json!({"type": "toggle", "id": "displays", "icon": "video-display-symbolic", "title": t("Displays"),
            "sub": sub, "on": on > 1, "menu": {"type": "box", "children": items}}))
    }

    fn on_event(&mut self, host: &Host, _: &str, node: &str, _: &str, value: &str) {
        let r = match node.split_once(':') {
            // the toggle: both screens side by side, or the laptop's alone
            None if node == "displays" => self.set(if value == "true" { "extend" } else { "laptop" }),
            Some(("mode", mode)) => self.set(mode),
            Some(("profile", name)) => switch(name),
            _ => return,
        };
        if let Err(e) = r {
            say(host, e);
        }
        host.kick();
    }
}

fn main() {
    ostrov_plugin::run(Displays { side: "right".into(), watching: false });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `hyprctl -j monitors all` on a laptop with its screen alone, recorded.
    const RECORDED: &str = include_str!("../tests/monitors.json");

    fn dell(enabled: bool, mirror: &str) -> Mon {
        let (mode, mirror) = ("3840x2160@60.00".into(), mirror.into());
        Mon { name: "DP-3".into(), enabled, mirror, mode, scale: 1.5 }
    }

    #[test]
    fn modes() {
        let mut mons = parse(RECORDED);
        let laptop = Mon { name: "eDP-1".into(), enabled: true, mirror: "".into(), mode: "3120x2080@120.00".into(),
            scale: 2.0 };
        assert_eq!(mons, std::slice::from_ref(&laptop));
        assert_eq!(current(&mons), None);
        assert!(rules("extend", &mons, "right").is_err());
        assert!(parse("").is_empty() && split(&[]).is_none());

        // the Dell plugged in, Hyprland's own rules putting it on beside the laptop
        mons.insert(0, dell(true, ""));
        assert_eq!(current(&mons), Some("extend"));
        assert_eq!(rules("extend", &mons, "left").unwrap(),
            ["eDP-1,3120x2080@120.00,0x0,2", "DP-3,3840x2160@60.00,auto-left,1.5"]);
        assert_eq!(rules("laptop", &mons, "right").unwrap(), ["eDP-1,3120x2080@120.00,0x0,2", "DP-3,disable"]);
        assert_eq!(rules("external", &mons, "right").unwrap(), ["DP-3,3840x2160@60.00,0x0,1.5", "eDP-1,disable"]);
        assert_eq!(rules("mirror", &mons, "right").unwrap(),
            ["eDP-1,3120x2080@120.00,0x0,2", "DP-3,preferred,auto,auto,mirror,eDP-1"]);
        assert!(rules("sideways", &mons, "right").is_err());

        // mirroring, the Dell's mode is not its own: back to its preferred one
        mons[0] = dell(true, "eDP-1");
        assert_eq!(current(&mons), Some("mirror"));
        assert_eq!(rules("extend", &mons, "below").unwrap()[1], "DP-3,preferred,auto-down,auto");
        mons[0] = dell(false, "");
        assert_eq!(current(&mons), Some("laptop"));
        mons[1].enabled = false;
        mons[0].enabled = true;
        assert_eq!(current(&mons), Some("external"));
        assert_eq!(rules("laptop", &mons, "right").unwrap(), ["eDP-1,preferred,0x0,auto", "DP-3,disable"]);
    }

    #[test]
    fn kanshi_config() {
        let config = "profile desk {\n  output eDP-1 disable\n}\nprofile \"on the go\" {\n}\nprofile {\n}\n\
            # profile commented {\nprofiles x\ninclude ~/other\n  profile  docked{\n}\n";
        assert_eq!(kanshi_profiles(config), ["desk", "on the go", "docked"]);
    }
}
