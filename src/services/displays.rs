//! The monitors through Hyprland (its socket's j/monitors all, keyword monitor): each one's mode, place, scale,
//! whether it is on or mirrors another; the layout set from the quick settings or `ostrov displays ...`. Layouts
//! kept by name as profiles in ~/.config/ostrov/displays.toml, a file of ostrov's own rather than a part of
//! config.toml: that one is the user's, in the dotfiles, commented by hand, and a save would write it over. A
//! profile names its monitors by description (make, model, serial), which stay what they are from dock to dock
//! where the connector names do not; when the monitors plugged in become exactly a profile's, it is applied.
//! Nothing of this outside Hyprland: no monitors, every command an error.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{Kick, Res, USAGE};
use crate::wm::{hypr_socket, hyprctl};

/// A monitor as Hyprland tells it.
#[derive(Clone, Default, Debug)]
struct Mon {
    name: String,
    desc: String,
    width: i64,
    height: i64,
    refresh: f64,
    scale: f64,
    x: i64,
    y: i64,
    transform: i64,
    enabled: bool,
    /// the name of the monitor this one mirrors, "" for none
    mirror: String,
}

impl Mon {
    fn from(v: &Value) -> Mon {
        let mirror = v["mirrorOf"].as_str().filter(|m| *m != "none").unwrap_or("").to_string();
        Mon {
            name: v["name"].as_str().unwrap_or("").into(),
            desc: v["description"].as_str().unwrap_or("").into(),
            width: v["width"].as_i64().unwrap_or(0),
            height: v["height"].as_i64().unwrap_or(0),
            refresh: v["refreshRate"].as_f64().unwrap_or(0.0),
            scale: v["scale"].as_f64().unwrap_or(1.0),
            x: v["x"].as_i64().unwrap_or(0),
            y: v["y"].as_i64().unwrap_or(0),
            transform: v["transform"].as_i64().unwrap_or(0),
            enabled: !v["disabled"].as_bool().unwrap_or(false),
            mirror,
        }
    }
    /// What a profile knows it by: its description, its connector's name where it has none.
    fn key(&self) -> &str {
        if self.desc.is_empty() { &self.name } else { &self.desc }
    }
    fn mode(&self) -> String {
        format!("{}x{}@{:.2}", self.width, self.height, self.refresh)
    }
    /// Its size on the layout: the mode scaled down, turned a quarter by an odd transform.
    fn logical(&self) -> (i64, i64) {
        logical(self.width, self.height, self.scale, self.transform)
    }
}

fn logical(w: i64, h: i64, scale: f64, transform: i64) -> (i64, i64) {
    let (w, h) = ((w as f64 / scale).round() as i64, (h as f64 / scale).round() as i64);
    if transform % 2 == 1 { (h, w) } else { (w, h) }
}

/// Every monitor Hyprland has, the disabled and mirroring ones too, with the JSON it told them in.
fn monitors() -> Vec<(Mon, Value)> {
    let list: Value = serde_json::from_str(&hyprctl("j/monitors all")).unwrap_or_default();
    list.as_array().into_iter().flatten().map(|v| (Mon::from(v), v.clone())).collect()
}

/// The monitor the others are placed beside: the first one on that mirrors none, other than skip.
fn main_of<'a>(mons: &'a [Mon], skip: &str) -> Option<&'a Mon> {
    mons.iter().find(|m| m.enabled && m.mirror.is_empty() && m.name != skip)
}

/// A monitor of size w by h at scale placed left of, right of, above or below main, their tops or left edges
/// lined up; any other word is a place as Hyprland takes it ("0x0", "auto"), passed on as it is.
fn place(rel: &str, main: Option<&Mon>, mode: &str, scale: f64) -> String {
    let size = mode.split('@').next().and_then(|s| s.split_once('x'));
    let size = size.and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
    let (Some(m), Some((w, h))) = (main, size) else {
        return if matches!(rel, "left" | "right" | "above" | "below") { "auto".into() } else { rel.into() };
    };
    let (w, h) = logical(w, h, scale, 0);
    let (mw, mh) = m.logical();
    let (x, y) = match rel {
        "left" => (m.x - w, m.y),
        "right" => (m.x + mw, m.y),
        "above" => (m.x, m.y - h),
        "below" => (m.x, m.y + mh),
        _ => return rel.into(),
    };
    format!("{x}x{y}")
}

/// Hyprland's monitor rule: NAME,MODE,POSITION,SCALE.
fn rule(name: &str, mode: &str, pos: &str, scale: f64) -> String {
    format!("{name},{mode},{pos},{scale}")
}

/// A monitor rule set; Hyprland answers "ok" or what is wrong with it.
fn keyword(rule: &str) -> Res {
    let out = hyprctl(&format!("keyword monitor {rule}"));
    match out.trim() {
        "ok" => Ok(()),
        "" => Err("not under Hyprland".into()),
        e => Err(e.into()),
    }
}

/// A monitor of a profile: what it was set to when saved.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Saved {
    description: String,
    enabled: bool,
    mode: String,
    position: String,
    scale: f64,
    #[serde(default)]
    transform: i64,
    /// the description of the monitor it mirrors, "" for none
    #[serde(default)]
    mirror: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Profile {
    monitors: Vec<Saved>,
}

type Profiles = BTreeMap<String, Profile>;

fn file() -> std::path::PathBuf {
    crate::hub::home().join(".config/ostrov/displays.toml")
}

fn load() -> Profiles {
    std::fs::read_to_string(file()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
}

fn store(p: &Profiles) -> Res {
    let text = toml::to_string(p).map_err(|e| e.to_string())?;
    if let Some(dir) = file().parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(file(), text).map_err(|e| e.to_string())
}

/// The layout now, as a profile keeps it.
fn snapshot(mons: &[Mon]) -> Profile {
    let desc = |name: &str| mons.iter().find(|m| m.name == name).map_or(String::new(), |m| m.key().to_string());
    let monitors = mons
        .iter()
        .map(|m| Saved {
            description: m.key().into(),
            enabled: m.enabled,
            mode: m.mode(),
            position: format!("{}x{}", m.x, m.y),
            scale: m.scale,
            transform: m.transform,
            mirror: desc(&m.mirror),
        })
        .collect();
    Profile { monitors }
}

/// Whether a profile is of exactly these monitors, by description, in whatever order.
fn matches(p: &Profile, mons: &[Mon]) -> bool {
    let mut a: Vec<&str> = p.monitors.iter().map(|s| s.description.as_str()).collect();
    let mut b: Vec<&str> = mons.iter().map(Mon::key).collect();
    a.sort();
    b.sort();
    !a.is_empty() && a == b
}

/// The first profile, by name, of exactly these monitors.
fn pick<'a>(ps: &'a Profiles, mons: &[Mon]) -> Option<&'a str> {
    ps.iter().find(|(_, p)| matches(p, mons)).map(|(n, _)| n.as_str())
}

/// A profile's rules for these monitors, by their names now: the monitors on first, the mirrors after them and
/// the ones off last, so the screen is never left with none on midway. None when a monitor is missing.
fn rules(p: &Profile, mons: &[Mon]) -> Option<Vec<String>> {
    let name = |desc: &str| mons.iter().find(|m| m.key() == desc).map(|m| m.name.clone());
    let mut out = Vec::new();
    for pass in 0..3 {
        for s in &p.monitors {
            let n = name(&s.description)?;
            let r = match (s.enabled, s.mirror.is_empty()) {
                (true, true) if pass == 0 => rule(&n, &s.mode, &s.position, s.scale) + &transform(s.transform),
                (true, false) if pass == 1 => {
                    format!("{},mirror,{}", rule(&n, &s.mode, "auto", s.scale), name(&s.mirror)?)
                }
                (false, _) if pass == 2 => format!("{n},disable"),
                _ => continue,
            };
            out.push(r);
        }
    }
    Some(out)
}

fn transform(t: i64) -> String {
    if t == 0 { String::new() } else { format!(",transform,{t}") }
}

fn apply(p: &Profile, mons: &[Mon]) -> Res {
    let rules = rules(p, mons).ok_or("the profile's monitors are not all connected")?;
    rules.iter().try_for_each(|r| keyword(r))
}

/// The monitors, the profiles saved and the one that is of the monitors connected ("" for none).
pub fn state() -> Value {
    let all = monitors();
    let mons: Vec<Mon> = all.iter().map(|(m, _)| m.clone()).collect();
    let main = main_of(&mons, "").map(|m| m.name.clone()).unwrap_or_default();
    let list: Vec<Value> = all
        .iter()
        .map(|(m, v)| {
            let mut modes: Vec<String> = Vec::new();
            for mode in v["availableModes"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                let mode = mode.trim_end_matches("Hz").to_string();
                if !modes.contains(&mode) {
                    modes.push(mode);
                }
            }
            json!({
                "name": m.name,
                "description": m.desc,
                "make": v["make"],
                "model": v["model"],
                "width": m.width,
                "height": m.height,
                "refresh": m.refresh,
                "mode": m.mode(),
                "scale": m.scale,
                "x": m.x,
                "y": m.y,
                "transform": m.transform,
                "enabled": m.enabled,
                "mirrorOf": m.mirror,
                "main": m.name == main,
                "availableModes": modes,
            })
        })
        .collect();
    let profiles = load();
    json!({
        "monitors": list,
        "profiles": profiles.keys().collect::<Vec<_>>(),
        "profile": pick(&profiles, &mons).unwrap_or(""),
    })
}

/// displays set NAME MODE POSITION SCALE (POSITION XxY, auto, or left, right, above, below the main monitor),
/// on NAME, off NAME, mirror NAME OF; save NAME, load NAME, delete NAME.
pub async fn cmd(args: &[&str]) -> Res {
    let mons: Vec<Mon> = monitors().into_iter().map(|(m, _)| m).collect();
    let known = |n: &str| mons.iter().any(|m| m.name == n).then_some(()).ok_or(format!("no monitor {n}"));
    match args {
        ["set", name, mode, pos, scale] => {
            known(name)?;
            let scale: f64 = scale.parse().map_err(|_| USAGE.to_string())?;
            keyword(&rule(name, mode, &place(pos, main_of(&mons, name), mode, scale), scale))
        }
        ["on", name] => {
            known(name)?;
            keyword(&rule(name, "preferred", "auto", 1.0))
        }
        ["off", name] => {
            known(name)?;
            if main_of(&mons, name).is_none() {
                return Err(format!("{name} is the only screen on"));
            }
            keyword(&format!("{name},disable"))
        }
        ["mirror", name, of] => {
            known(name)?;
            known(of)?;
            keyword(&format!("{name},preferred,auto,1,mirror,{of}"))
        }
        ["save", name] => {
            if mons.is_empty() {
                return Err("no monitors".into());
            }
            let mut ps = load();
            ps.insert(name.to_string(), snapshot(&mons));
            store(&ps)
        }
        ["load", name] => apply(load().get(*name).ok_or(format!("no profile {name}"))?, &mons),
        ["delete", name] => {
            let mut ps = load();
            ps.remove(*name).ok_or(format!("no profile {name}"))?;
            store(&ps)
        }
        _ => Err(USAGE.into()),
    }
}

/// A kick on every monitor plugged in or out, and the profile of the monitors connected then applied: half a
/// second on, once Hyprland has set the newcomer up by its own rules, and only when the set of monitors is not
/// the one last seen (a profile's own rules turning a monitor off or on make events of their own). Hyprland's
/// event socket read on a thread of its own, as keymap's.
pub async fn events(kick: Kick) {
    let Some(sock) = hypr_socket(".socket2.sock") else { return };
    let _ = tokio::task::spawn_blocking(move || {
        let Ok(c) = UnixStream::connect(sock) else { return };
        let keys = || {
            let mut k: Vec<String> = monitors().into_iter().map(|(m, _)| m.key().to_string()).collect();
            k.sort();
            k
        };
        let mut seen = keys();
        for line in BufReader::new(c).lines() {
            let Ok(line) = line else { return };
            if !(line.starts_with("monitoraddedv2>>") || line.starts_with("monitorremoved>>")) {
                continue;
            }
            std::thread::sleep(Duration::from_millis(500));
            let now = keys();
            if now != seen {
                seen = now;
                let mons: Vec<Mon> = monitors().into_iter().map(|(m, _)| m).collect();
                let ps = load();
                if let Some(name) = pick(&ps, &mons) {
                    if let Err(e) = apply(&ps[name], &mons) {
                        eprintln!("ostrov: displays: {name}: {e}");
                    }
                }
            }
            if kick.send_blocking(()).is_err() {
                return;
            }
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(name: &str, desc: &str, w: i64, h: i64, scale: f64, x: i64) -> Mon {
        let (name, desc) = (name.into(), desc.into());
        Mon { name, desc, width: w, height: h, refresh: 60.0, scale, x, enabled: true, ..Mon::default() }
    }

    #[test]
    fn lines() {
        let laptop = mon("eDP-1", "EDO EDO14.55", 3120, 2080, 2.0, 0);
        assert_eq!(rule("HDMI-A-1", "2560x1440@59.95", "0x0", 1.25), "HDMI-A-1,2560x1440@59.95,0x0,1.25");
        assert_eq!(rule("HDMI-A-1", "1920x1080@60.00", "auto", 2.0), "HDMI-A-1,1920x1080@60.00,auto,2");
        // beside the laptop's 1560x1040 logical screen
        assert_eq!(place("right", Some(&laptop), "2560x1440@59.95", 1.0), "1560x0");
        assert_eq!(place("left", Some(&laptop), "2560x1440@59.95", 1.25), "-2048x0");
        assert_eq!(place("above", Some(&laptop), "1920x1080@60", 1.0), "0x-1080");
        assert_eq!(place("below", Some(&laptop), "1920x1080@60", 1.0), "0x1040");
        assert_eq!(place("10x20", Some(&laptop), "1920x1080@60", 1.0), "10x20");
        assert_eq!(place("left", None, "preferred", 1.0), "auto");
        // a quarter turn swaps the sides
        assert_eq!(logical(2560, 1440, 1.0, 1), (1440, 2560));
    }

    #[test]
    fn profiles() {
        let mut ext = mon("DP-3", "Dell U2720Q", 3840, 2160, 1.5, 1560);
        let laptop = mon("eDP-1", "EDO EDO14.55", 3120, 2080, 2.0, 0);
        let mut ps = Profiles::new();
        ps.insert("desk".into(), snapshot(&[laptop.clone(), ext.clone()]));
        ps.insert("alone".into(), snapshot(std::slice::from_ref(&laptop)));
        assert_eq!(pick(&ps, &[ext.clone(), laptop.clone()]), Some("desk"));
        assert_eq!(pick(&ps, std::slice::from_ref(&laptop)), Some("alone"));
        assert_eq!(pick(&ps, &[laptop.clone(), mon("DP-1", "Other", 1920, 1080, 1.0, 0)]), None);
        assert_eq!(pick(&Profiles::new(), &[laptop.clone()]), None);

        // the dock names the Dell DP-5 now: the rules follow its description; laptop off after the others on
        let mut desk = snapshot(&[laptop.clone(), ext.clone()]);
        desk.monitors[0].enabled = false;
        ext.name = "DP-5".into();
        assert_eq!(
            rules(&desk, &[laptop.clone(), ext.clone()]).unwrap(),
            vec!["DP-5,3840x2160@60.00,1560x0,1.5", "eDP-1,disable"]
        );
        // a mirror after the screen it mirrors, by that one's name now
        let mut mirrored = snapshot(&[laptop.clone(), ext.clone()]);
        mirrored.monitors[1].mirror = "EDO EDO14.55".into();
        assert_eq!(
            rules(&mirrored, &[ext.clone(), laptop.clone()]).unwrap(),
            vec!["eDP-1,3120x2080@60.00,0x0,2", "DP-5,3840x2160@60.00,auto,1.5,mirror,eDP-1"]
        );
        assert!(rules(&desk, std::slice::from_ref(&laptop)).is_none());

        // through the file's TOML and back
        let text = toml::to_string(&ps).unwrap();
        let back: Profiles = toml::from_str(&text).unwrap();
        assert_eq!(back["desk"].monitors, ps["desk"].monitors);
    }

    /// The state as it is here: cargo test -- --ignored --nocapture displays_state
    #[test]
    #[ignore]
    fn displays_state() {
        println!("{}", state());
    }
}
