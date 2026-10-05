//! The menus, in wmenu, of what is rarely needed (Wi-Fi, Bluetooth, the sound, the power), and what a click on a
//! status block does. A menu's items are made from the services' state, each a label and the command it runs:
//! a service's (wifi connect SSID, as `ostrov wifi ...` takes it), `menu NAME` for another menu, `ask ...` for
//! the command with a passphrase asked first, or else a program and its arguments.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::i18n::{fill, t};
use crate::services::{Ctx, Res};
use crate::{audio, battery, bt, power, status, wifi};

#[derive(Debug, PartialEq)]
pub struct Item {
    pub label: String,
    pub cmd: Vec<String>,
}

fn item(label: impl Into<String>, cmd: &[&str]) -> Item {
    Item { label: label.into(), cmd: cmd.iter().map(|w| w.to_string()).collect() }
}

/// A mark before the label of what is on now, the room of one before the rest, so the labels line up.
fn mark(on: bool, label: &str) -> String {
    format!("{} {label}", if on { "●" } else { " " })
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

fn list(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or_default()
}

pub fn wifi_items(st: &Value) -> Vec<Item> {
    if st["on"] != true {
        return vec![item(t("Turn Wi-Fi on"), &["wifi", "on"])];
    }
    let mut items: Vec<Item> = Vec::new();
    for n in list(&st["networks"]) {
        let (ssid, bars) = (s(n, "ssid"), n["signal"].as_u64().unwrap_or(0) as usize);
        let lock = if s(n, "security") == "open" || n["known"] == true { " " } else { "🔒" };
        let bars: String = "▂▄▆█".chars().take(bars).collect();
        let label = mark(n["connected"] == true, &format!("{bars:<4} {lock} {ssid}"));
        let cmd: &[&str] = match () {
            _ if n["connected"] == true => &["wifi", "disconnect"],
            _ if lock == " " => &["wifi", "connect", ssid],
            _ => &["ask", "wifi", "connect", ssid],
        };
        if !ssid.is_empty() && !items.iter().any(|i| i.label == label) {
            items.push(item(label, cmd));
        }
    }
    items.push(item(t("Scan"), &["wifi", "scan"]));
    items.push(item(t("Turn Wi-Fi off"), &["wifi", "off"]));
    items
}

pub fn bt_items(st: &Value) -> Vec<Item> {
    if st["on"] != true {
        return vec![item(t("Turn Bluetooth on"), &["bt", "on"])];
    }
    let mut items: Vec<Item> = list(&st["devices"])
        .iter()
        .map(|d| {
            let (addr, name) = (s(d, "address"), s(d, "name"));
            match (d["paired"] == true, d["connected"] == true) {
                (_, true) => item(mark(true, name), &["bt", "disconnect", addr]),
                (true, false) => item(mark(false, name), &["bt", "connect", addr]),
                _ => item(mark(false, &fill(t("{} (pair)"), &[&name])), &["bt", "pair", addr]),
            }
        })
        .collect();
    items.push(item(t("Scan for 20 s"), &["bt", "scan"]));
    items.push(item(t("Turn Bluetooth off"), &["bt", "off"]));
    items
}

pub fn audio_items(st: &Value) -> Vec<Item> {
    let mut items = Vec::new();
    for (key, what) in [("sinks", t("Output")), ("sources", t("Input"))] {
        for d in list(&st[key]) {
            let label = mark(d["def"] == true, &format!("{what}: {}", s(d, "name")));
            let n = |k: &str| d[k].as_i64().unwrap_or(0).to_string();
            let cmd = match d["id"].as_i64() {
                Some(id) if id > 0 => vec!["wpctl".into(), "set-default".into(), id.to_string()],
                _ => {
                    let route = |i: usize| d["route"][i].as_i64().unwrap_or(0).to_string();
                    vec!["audio".into(), "port".into(), n("card"), n("profile"), route(0), route(1)]
                }
            };
            items.push(Item { label, cmd });
        }
    }
    let muted = st["muted"] == true;
    let mute = ["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"];
    items.push(item(t(if muted { "Unmute the sound" } else { "Mute the sound" }), &mute));
    items.push(mic_item(st));
    match s(st, "headset") {
        "handsfree" => items.push(item(t("Headset: headphones (no mic)"), &["headset"])),
        "headphones" => items.push(item(t("Headset: handsfree (with the mic)"), &["headset"])),
        _ => {}
    }
    items.extend(privacy_items(st));
    items
}

/// The default mic muted, or back.
fn mic_item(st: &Value) -> Item {
    item(t(if st["micMuted"] == true { "Unmute the mic" } else { "Mute the mic" }), &["audio", "mic-mute"])
}

/// The screen's share and the cameras' streams to stop.
fn privacy_items(st: &Value) -> Vec<Item> {
    let mut items = Vec::new();
    if st["screen"] == true {
        items.push(item(t("Stop sharing the screen"), &["audio", "stop-screen"]));
    }
    for c in list(&st["camStreams"]) {
        items.push(item(fill(t("Stop the camera: {}"), &[&s(c, "name")]), &["audio", "stop-camera", s(c, "name")]));
    }
    items
}

/// The profile after the active one, round.
pub fn next_profile(st: &Value) -> Option<String> {
    let all: Vec<&str> = list(&st["profiles"]).iter().filter_map(Value::as_str).collect();
    let i = all.iter().position(|p| *p == s(st, "active")).map_or(0, |i| i + 1);
    all.get(i % all.len().max(1)).map(|p| p.to_string())
}

fn profile_name(p: &str) -> &str {
    match p {
        "power-saver" => t("Power Saver"),
        "balanced" => t("Balanced"),
        "performance" => t("Performance"),
        p => p,
    }
}

pub fn power_items(st: &Value, limit: &Value) -> Vec<Item> {
    let mut items: Vec<Item> = list(&st["profiles"])
        .iter()
        .filter_map(Value::as_str)
        .map(|p| item(mark(s(st, "active") == p, profile_name(p)), &["power", "set", p]))
        .collect();
    if let Some(end) = limit["end"].as_u64() {
        for n in [80, 100] {
            let label = mark(end == n, &fill(t("Charge limit {}%"), &[&n]));
            items.push(item(label, &["battery", "limit", &n.to_string()]));
        }
    }
    items.push(item(t("Suspend"), &["systemctl", "suspend"]));
    items.push(item(t("Restart…"), &["systemctl", "reboot"]));
    items.push(item(t("Power Off…"), &["systemctl", "poweroff"]));
    items
}

/// The VPNs of the user's own scripts, when on PATH: vless's state ("off" or how it runs), openvpn-ctl's ("off"
/// or the profile up) and its profiles.
pub struct Vpn {
    pub vless: Option<String>,
    pub openvpn: Option<(String, Vec<String>)>,
}

pub fn main_items(notes: Option<(usize, bool)>, audio: &Value, vpn: &Vpn) -> Vec<Item> {
    let mut items = vec![
        item(t("Wi-Fi…"), &["menu", "wifi"]),
        item(t("Bluetooth…"), &["menu", "bt"]),
        item(t("Sound…"), &["menu", "audio"]),
        item(t("Power…"), &["menu", "power"]),
    ];
    if let Some((n, dnd)) = notes {
        let label = if dnd { t("Turn Do Not Disturb off") } else { t("Turn Do Not Disturb on") };
        items.push(item(label, &["makoctl", "mode", "-t", "do-not-disturb"]));
        if n > 0 {
            items.push(item(fill(t("Dismiss the notifications ({})"), &[&n]), &["makoctl", "dismiss", "-a"]));
        }
    }
    if status::privacy(audio) {
        items.push(mic_item(audio));
        items.extend(privacy_items(audio));
    }
    match vpn.vless.as_deref() {
        Some("off") => items.push(item(t("VLESS on"), &["vless", "on", "tun"])),
        Some(how) => items.push(item(fill(t("VLESS off ({})"), &[&how]), &["vless", "off"])),
        None => {}
    }
    if let Some((up, profiles)) = &vpn.openvpn {
        for p in profiles.iter().filter(|p| *p != up) {
            items.push(item(fill(t("OpenVPN: {}"), &[p]), &["openvpn-ctl", "on", p]));
        }
        if up != "off" {
            items.push(item(fill(t("OpenVPN off ({})"), &[up]), &["openvpn-ctl", "off"]));
        }
    }
    items
}

/// A menu for each name, a status block's by the block's (the network's the Wi-Fi's, the battery's the power's...).
fn menu_of(name: &str) -> &str {
    match name {
        "net" | "wifi" => "wifi",
        "mute" | "privacy" | "audio" => "audio",
        "battery" | "power" => "power",
        "bt" => "bt",
        _ => "main",
    }
}

pub const MENUS: &str = "wifi|bt|audio|power|main";

/// A program's first line of output, None if it is not there or fails.
fn output(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).stderr(Stdio::null()).output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn items(c: &Ctx, name: &str) -> (&'static str, Vec<Item>) {
    match name {
        "wifi" => (t("Wi-Fi"), wifi_items(&wifi::service::state(c).await)),
        "bt" => (t("Bluetooth"), bt_items(&bt::service::state(c).await)),
        "audio" => (t("Sound"), audio_items(&audio::service::state().await)),
        "power" => (t("Power"), power_items(&power::service::state(c).await, &battery::limit::state())),
        _ => {
            let vpn = Vpn {
                vless: output("vless", &["status"]).and_then(|s| s.lines().next().map(String::from)),
                openvpn: output("openvpn-ctl", &["status"]).map(|up| {
                    let list = output("openvpn-ctl", &["list"]).unwrap_or_default();
                    (up.trim().to_string(), list.lines().map(String::from).collect())
                }),
            };
            let (notes, audio) = tokio::join!(status::notes(c), audio::service::state());
            ("ostrov", main_items(notes, &audio, &vpn))
        }
    }
}

/// wmenu with these lines, what was picked (or typed) back; None on Escape.
fn wmenu(args: &[&str], lines: &str) -> Option<String> {
    let mut child = Command::new("wmenu").args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().ok()?;
    let _ = child.stdin.take()?.write_all(lines.as_bytes());
    let out = child.wait_with_output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string())
}

/// The menu NAME (a status block's name its menu's) shown, the item picked run; a submenu picked shown in turn.
pub async fn menu(c: &Ctx, name: &str) -> Res {
    let mut name = menu_of(name).to_string();
    loop {
        let (prompt, items) = items(c, &name).await;
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        let Some(pick) = wmenu(&["-l", "15", "-p", prompt], &labels.join("\n")) else { return Ok(()) };
        let Some(it) = items.into_iter().find(|i| i.label == pick) else { return Ok(()) };
        let words: Vec<&str> = it.cmd.iter().map(String::as_str).collect();
        match words[..] {
            ["menu", next] => name = next.to_string(),
            ["ask", ref cmd @ ..] => {
                let Some(pass) = wmenu(&["-P", "-p", t("Password")], "") else { return Ok(()) };
                return run(c, cmd, Some(pass)).await;
            }
            ref cmd => return run(c, cmd, None).await,
        }
    }
}

/// An item's command run: a service's, else a program to its end.
pub async fn run(c: &Ctx, cmd: &[&str], input: Option<String>) -> Res {
    match cmd {
        ["wifi", rest @ ..] => wifi::service::cmd(c, rest, input).await,
        ["bt", rest @ ..] => bt::service::cmd(c, rest).await,
        ["audio", rest @ ..] => audio::service::cmd(rest).await,
        ["headset"] => audio::service::headset().await,
        ["power", rest @ ..] => power::service::cmd(c, rest).await,
        ["battery", "limit", n] => battery::limit::set(n.parse().map_err(|_| format!("not a percent: {n}"))?),
        [prog, args @ ..] => {
            let st = tokio::process::Command::new(prog).args(args).status().await.map_err(|e| format!("{prog}: {e}"))?;
            if st.success() { Ok(()) } else { Err(format!("{prog}: {st}")) }
        }
        [] => Ok(()),
    }
}

/// What a click on a status block does: the battery's the next power profile, the notifications' Do Not Disturb
/// flipped, the privacy dot's the mic muted, the mute's the sound back; the network's and the clock's a menu.
pub async fn click(c: &Ctx, block: &str) -> Res {
    match block {
        "battery" => {
            let next = next_profile(&power::service::state(c).await).ok_or("no power profiles")?;
            run(c, &["power", "set", &next], None).await
        }
        "notes" => run(c, &["makoctl", "mode", "-t", "do-not-disturb"], None).await,
        "privacy" => run(c, &["audio", "mic-mute"], None).await,
        "mute" => run(c, &["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"], None).await,
        "net" | "clock" => menu(c, block).await,
        _ => Err(format!("ostrov-ctl click: no block {block}")),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn cmd(items: &[Item], label: &str) -> String {
        items.iter().find(|i| i.label.contains(label)).map(|i| i.cmd.join(" ")).unwrap_or_default()
    }

    #[test]
    fn wifi() {
        let st = json!({"on": true, "networks": [
            {"ssid": "Home", "signal": 4, "security": "psk", "known": true, "connected": true},
            {"ssid": "Cafe", "signal": 2, "security": "open", "known": false, "connected": false},
            {"ssid": "Work", "signal": 1, "security": "psk", "known": false, "connected": false},
            {"ssid": "Old", "signal": 3, "security": "psk", "known": true, "connected": false}]});
        let items = wifi_items(&st);
        assert_eq!(items[0].label, "● ▂▄▆█   Home");
        assert_eq!(cmd(&items, "Home"), "wifi disconnect");
        assert_eq!(cmd(&items, "Cafe"), "wifi connect Cafe");
        assert_eq!(cmd(&items, "Work"), "ask wifi connect Work");
        assert_eq!(cmd(&items, "Old"), "wifi connect Old");
        assert_eq!(items.last().map(|i| i.cmd.join(" ")), Some("wifi off".into()));
        assert_eq!(wifi_items(&json!({"on": false})), vec![item("Turn Wi-Fi on", &["wifi", "on"])]);
    }

    #[test]
    fn bt() {
        let st = json!({"on": true, "devices": [
            {"address": "A", "name": "Buds", "paired": true, "connected": true},
            {"address": "B", "name": "Mouse", "paired": true, "connected": false},
            {"address": "C", "name": "Speaker", "paired": false, "connected": false}]});
        let items = bt_items(&st);
        assert_eq!(cmd(&items, "Buds"), "bt disconnect A");
        assert_eq!(cmd(&items, "Mouse"), "bt connect B");
        assert_eq!(cmd(&items, "Speaker (pair)"), "bt pair C");
    }

    #[test]
    fn audio() {
        let st = json!({"sinks": [{"id": 40, "name": "Speakers", "def": true},
            {"id": 0, "name": "Speaker", "def": false, "card": 48, "profile": 2, "route": [1, 3]}],
            "sources": [{"id": 41, "name": "Mic", "def": false}], "muted": true, "headset": "handsfree",
            "screen": true, "camStreams": [{"id": 90, "name": "zoom"}]});
        let items = audio_items(&st);
        assert_eq!(items[0].label, "● Output: Speakers");
        assert_eq!(cmd(&items, "Output: Speakers"), "wpctl set-default 40");
        assert_eq!(items[1].cmd.join(" "), "audio port 48 2 1 3");
        assert_eq!(cmd(&items, "Input: Mic"), "wpctl set-default 41");
        assert_eq!(cmd(&items, "Unmute the sound"), "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle");
        assert_eq!(cmd(&items, "Headset"), "headset");
        assert_eq!(cmd(&items, "sharing"), "audio stop-screen");
        assert_eq!(cmd(&items, "camera"), "audio stop-camera zoom");
    }

    #[test]
    fn power() {
        let st = json!({"active": "balanced", "profiles": ["power-saver", "balanced", "performance"]});
        let items = power_items(&st, &json!({"end": 80, "start": 75, "writable": true}));
        assert_eq!(items[1].label, "● Balanced");
        assert_eq!(cmd(&items, "Performance"), "power set performance");
        assert_eq!(cmd(&items, "● Charge limit 80%"), "battery limit 80");
        assert_eq!(cmd(&items, "Charge limit 100%"), "battery limit 100");
        assert_eq!(cmd(&items, "Suspend"), "systemctl suspend");
        assert_eq!(power_items(&json!({}), &Value::Null).len(), 3);
        assert_eq!(next_profile(&st).as_deref(), Some("performance"));
        let two = json!({"active": "performance", "profiles": ["balanced", "performance"]});
        assert_eq!(next_profile(&two).as_deref(), Some("balanced"));
        assert_eq!(next_profile(&json!({})), None);
    }

    #[test]
    fn main() {
        let openvpn = Some(("work".into(), vec!["work".into(), "home".into()]));
        let vpn = Vpn { vless: Some("tun rule PROXY".into()), openvpn };
        let audio = json!({"micApps": [{"id": 1, "name": "x"}], "micMuted": false});
        let items = main_items(Some((2, true)), &audio, &vpn);
        assert_eq!(cmd(&items, "Wi-Fi…"), "menu wifi");
        assert_eq!(cmd(&items, "Do Not Disturb off"), "makoctl mode -t do-not-disturb");
        assert_eq!(cmd(&items, "Dismiss the notifications (2)"), "makoctl dismiss -a");
        assert_eq!(cmd(&items, "Mute the mic"), "audio mic-mute");
        assert_eq!(cmd(&items, "VLESS off"), "vless off");
        assert_eq!(cmd(&items, "OpenVPN: home"), "openvpn-ctl on home");
        assert_eq!(cmd(&items, "OpenVPN off (work)"), "openvpn-ctl off");
        assert!(!items.iter().any(|i| i.label == "OpenVPN: work"));
        let none = main_items(None, &json!({}), &Vpn { vless: Some("off".into()), openvpn: None });
        assert_eq!(none.len(), 5);
        assert_eq!(cmd(&none, "VLESS on"), "vless on tun");
    }

    #[test]
    fn blocks_menus() {
        let menus = ["net", "mute", "battery", "clock", "bt", "main"].map(menu_of);
        assert_eq!(menus, ["wifi", "audio", "power", "main", "bt", "main"]);
    }
}
