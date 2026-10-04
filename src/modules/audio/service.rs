//! The sound devices through pw-dump, the headset's profile through wpctl; the apps' streams too: the
//! ones recording (the mic, the camera, besides whatever holds a /dev/video* open) and the ones playing, each with
//! its level for the quick settings' mixer.

use std::collections::{HashMap, HashSet};
use std::io::BufReader;

use serde::Serialize;
use serde_json::Value;

use crate::services::{Kick, Res};

/// A PipeWire sink or source: its node id (for wpctl set-default), its name, whether it is the default. Or one
/// of a card's outputs or inputs in a profile not on now (the speaker while the headphones' profile is: UCM
/// has them in two): the card and the profile to switch to for it, its id 0.
#[derive(Serialize)]
struct Sound {
    id: i64,
    name: String,
    def: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    card: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<i64>,
    /// its route on the card (index, device), put on after the profile: WirePlumber may leave none on
    #[serde(skip_serializing_if = "Option::is_none")]
    route: Option<(i64, i64)>,
}

/// An app recording: its stream's node id, the app's name.
#[derive(Serialize)]
struct App {
    id: i64,
    name: String,
}

/// An app's stream playing: its node id, the app's name, its icon as the app says it ("" for none, its binary's
/// desktop entry then), and its level as the default sink's.
#[derive(Serialize)]
struct Stream {
    id: i64,
    name: String,
    icon: String,
    bin: String,
    #[serde(serialize_with = "level")]
    volume: f64,
}

/// The sinks and sources by name, so a list holds still as the default moves, and the Bluetooth headset's mode:
/// headphones (A2DP) or handsfree (HFP), "" without one. Then the default output's and input's level, 0 to 1 as
/// wpctl puts it, and whether muted; a whole level goes out as an integer, as Go's encoder writes a float.
#[derive(Serialize, Default)]
struct Audio {
    sinks: Vec<Sound>,
    sources: Vec<Sound>,
    headset: String,
    #[serde(serialize_with = "level")]
    volume: f64,
    muted: bool,
    #[serde(serialize_with = "level")]
    mic: f64,
    #[serde(rename = "micMuted")]
    mic_muted: bool,
    /// the apps taking the mic, the camera (by name: PipeWire's and the processes' with a /dev/video* open), and
    /// the streams playing
    #[serde(rename = "micApps")]
    mic_apps: Vec<App>,
    #[serde(rename = "camApps")]
    cam_apps: Vec<String>,
    /// the screen shared through the portal (its stream a Video/Source of no device: xdg-desktop-portal-hyprland's
    /// xdph-streaming-N), and the apps taking it, kept out of camApps
    screen: bool,
    #[serde(rename = "screenApps")]
    screen_apps: Vec<String>,
    streams: Vec<Stream>,
}

fn level<S: serde::Serializer>(f: &f64, s: S) -> Result<S::Ok, S::Error> {
    if f.fract() == 0.0 { s.serialize_i64(*f as i64) } else { s.serialize_f64(*f) }
}

/// A string at a path of keys, "" when it is not there.
fn str<'a>(v: &'a Value, path: &[&str]) -> &'a str {
    path.iter().try_fold(v, |v, k| v.get(k)).and_then(Value::as_str).unwrap_or("")
}

/// A number at a path of keys as an integer, 0 when it is not there.
fn num(v: &Value, path: &[&str]) -> i64 {
    path.iter().try_fold(v, |v, k| v.get(k)).and_then(Value::as_f64).unwrap_or(0.0) as i64
}

/// The first entry of an info.params list, Null without one.
fn param<'a>(o: &'a Value, name: &str) -> &'a Value {
    o.pointer(&format!("/info/params/{name}/0")).unwrap_or(&Value::Null)
}

/// An info.params list, empty without one.
fn params<'a>(o: &'a Value, name: &str) -> impl Iterator<Item = &'a Value> {
    o.pointer(&format!("/info/params/{name}")).and_then(Value::as_array).into_iter().flatten()
}

/// A Props param's level as wpctl puts it: the cube root of the first channel's volume, to two places.
fn cube_level(pr: &Value) -> f64 {
    pr["channelVolumes"][0].as_f64().map_or(0.0, |v| (v.cbrt() * 100.0).round() / 100.0)
}

/// An app's name as its stream says it: the app's, else its binary's, else the node's.
fn app_name(p: &Value) -> String {
    ["application.name", "application.process.binary", "node.name"]
        .iter()
        .map(|k| str(p, &[k]))
        .find(|n| !n.is_empty())
        .unwrap_or("")
        .into()
}

async fn pw_dump() -> Result<Vec<Value>, String> {
    let out = tokio::process::Command::new("pw-dump").output().await.map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(out.status.to_string());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

/// pw-dump read: the nodes of class Audio/Sink or Audio/Source (wpctl lists WirePlumber's internal Bluetooth nodes
/// as sinks, the headset's real one sitting under its filters), left out unless the default when every port of
/// theirs the card reports unplugged ("available": "no": an HDMI output with no display; a port ALSA cannot sense,
/// the speaker or a mic, says "unknown").
async fn audio_state() -> Result<Audio, String> {
    Ok(parse(&pw_dump().await?))
}

/// The sound's state out of pw-dump's objects. A stream records while running; one playing is listed whatever its
/// state, so the mixer's sliders stay put through a pause.
fn parse(objs: &[Value]) -> Audio {
    let mut a = Audio::default();
    let mut defaults = HashMap::new();
    let mut routes = HashMap::new();
    // the screens' streams, and what each node takes from: links, output node to input node
    let mut screens = std::collections::HashSet::new();
    let mut takes = HashMap::new();
    for o in objs {
        match str(o, &["type"]) {
            "PipeWire:Interface:Node" => {
                let p = &o["info"]["props"];
                if str(p, &["media.class"]) == "Video/Source" && p.get("device.id").is_none() {
                    screens.insert(num(o, &["id"]));
                }
            }
            "PipeWire:Interface:Link" => {
                takes.insert(num(o, &["info", "input-node-id"]), num(o, &["info", "output-node-id"]));
            }
            _ => {}
        }
    }
    a.screen = !screens.is_empty();
    for o in objs {
        match str(o, &["type"]) {
            "PipeWire:Interface:Metadata" if str(o, &["props", "metadata.name"]) == "default" => {
                for m in o["metadata"].as_array().into_iter().flatten() {
                    if let Some(v) = m["value"].as_object() {
                        defaults.insert(str(m, &["key"]), v.get("name").and_then(Value::as_str).unwrap_or(""));
                    }
                }
            }
            "PipeWire:Interface:Device" => {
                routes.insert(num(o, &["id"]), o);
                if a.headset.is_empty() && str(o, &["info", "props", "device.api"]) == "bluez5" {
                    let handsfree = str(param(o, "Profile"), &["name"]).starts_with("headset-head-unit");
                    a.headset = if handsfree { "handsfree" } else { "headphones" }.into();
                }
            }
            _ => {}
        }
    }
    for o in objs {
        if str(o, &["type"]) != "PipeWire:Interface:Node" {
            continue;
        }
        let p = &o["info"]["props"];
        let running = str(o, &["info", "state"]) == "running";
        let (sink, key) = match str(p, &["media.class"]) {
            "Stream/Input/Audio" if running => {
                a.mic_apps.push(App { id: num(o, &["id"]), name: app_name(p) });
                continue;
            }
            "Stream/Input/Video" if running => {
                let screen = takes.get(&num(o, &["id"])).is_some_and(|src| screens.contains(src));
                if screen { &mut a.screen_apps } else { &mut a.cam_apps }.push(app_name(p));
                continue;
            }
            "Stream/Output/Audio" => {
                a.streams.push(Stream {
                    id: num(o, &["id"]),
                    name: app_name(p),
                    icon: str(p, &["application.icon_name"]).into(),
                    bin: str(p, &["application.process.binary"]).into(),
                    volume: cube_level(param(o, "Props")),
                });
                continue;
            }
            "Audio/Sink" => (true, "default.audio.sink"),
            "Audio/Source" => (false, "default.audio.source"),
            _ => continue,
        };
        let def = str(p, &["node.name"]) == defaults.get(key).copied().unwrap_or("");
        let (mut plugged, mut any) = (false, false);
        let card = num(p, &["card.profile.device"]);
        for r in routes.get(&num(p, &["device.id"])).into_iter().flat_map(|d| params(d, "EnumRoute")) {
            if r["devices"].as_array().is_some_and(|ds| ds.iter().any(|x| x.as_f64() == Some(card as f64))) {
                any = true;
                plugged = plugged || str(r, &["available"]) != "no";
            }
        }
        if !def && any && !plugged {
            continue;
        }
        let mut name = str(p, &["node.nick"]);
        if name.is_empty() {
            name = str(p, &["node.description"]);
        }
        let list = if sink { &mut a.sinks } else { &mut a.sources };
        list.push(Sound { id: num(o, &["id"]), name: name.into(), def, card: None, profile: None, route: None });
        // the default's level
        let pr = param(o, "Props");
        if def && !pr.is_null() {
            let level = cube_level(pr);
            let mute = pr["mute"].as_bool().unwrap_or(false);
            if sink {
                (a.volume, a.muted) = (level, mute);
            } else {
                (a.mic, a.mic_muted) = (level, mute);
            }
        }
    }
    // the cards' outputs and inputs in their other profiles, not unplugged, as a switch to that profile
    for d in routes.values().filter(|d| str(d, &["info", "props", "device.api"]) == "alsa") {
        let now = num(param(d, "Profile"), &["index"]);
        for r in params(d, "EnumRoute") {
            let profiles: Vec<i64> = r["profiles"].as_array().into_iter().flatten().filter_map(Value::as_i64).collect();
            let Some(&profile) = profiles.first().filter(|_| !profiles.contains(&now)) else { continue };
            if str(r, &["available"]) == "no" {
                continue;
            }
            let list = if str(r, &["direction"]) == "Output" { &mut a.sinks } else { &mut a.sources };
            let name = str(r, &["description"]).to_string();
            let device = r["devices"].as_array().and_then(|ds| ds.first()).and_then(Value::as_i64).unwrap_or(0);
            if !list.iter().any(|s| s.name == name) {
                let route = Some((num(r, &["index"]), device));
                list.push(Sound { id: 0, name, def: false, card: Some(num(d, &["id"])), profile: Some(profile), route });
            }
        }
    }
    a.sinks.sort_by(|x, y| x.name.cmp(&y.name));
    a.sources.sort_by(|x, y| x.name.cmp(&y.name));
    a.streams.sort_by(|x, y| x.name.cmp(&y.name).then(x.id.cmp(&y.id)));
    a
}

/// The sound's state last read, and whether PipeWire has changed it since: every state the watcher sends (every
/// 3 s besides a change) would run a pw-dump of the whole graph otherwise, and the monitor (events) tells every
/// change that counts.
static LAST: std::sync::Mutex<Value> = std::sync::Mutex::new(Value::Null);
static STALE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// The processes holding a camera open, as the last scan (scan_cameras) found them.
static CAMERAS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub async fn state() -> Value {
    let mut v = if !STALE.swap(false, std::sync::atomic::Ordering::Relaxed) {
        LAST.lock().map(|v| v.clone()).unwrap_or_default()
    } else {
        let a = audio_state().await.unwrap_or_else(|e| {
            eprintln!("ostrov: audio: {e}");
            STALE.store(true, std::sync::atomic::Ordering::Relaxed);
            Audio::default()
        });
        let v = serde_json::to_value(a).unwrap_or_default();
        if let Ok(mut last) = LAST.lock() {
            *last = v.clone();
        }
        v
    };
    if let (Some(cams), Ok(procs)) = (v["camApps"].as_array_mut(), CAMERAS.lock()) {
        for p in procs.iter().map(|p| Value::from(p.as_str())) {
            if !cams.contains(&p) {
                cams.push(p);
            }
        }
    }
    v
}

/// The processes with a /dev/video* open looked for again, off the runtime's threads: a walk of every process's
/// fds, so the watcher runs it on its 3 s tick alone, not on PipeWire's every change. PipeWire's own processes are
/// left out, its camera's users being its streams; another user's processes cannot be looked into.
pub async fn scan_cameras() {
    let found = tokio::task::spawn_blocking(|| {
        let mut names: Vec<String> = Vec::new();
        for p in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
            let Ok(fds) = std::fs::read_dir(p.path().join("fd")) else { continue };
            let video = |f: std::fs::DirEntry| {
                std::fs::read_link(f.path()).is_ok_and(|l| l.to_string_lossy().starts_with("/dev/video"))
            };
            if !fds.flatten().any(video) {
                continue;
            }
            let comm = std::fs::read_to_string(p.path().join("comm")).unwrap_or_default().trim().to_string();
            if !comm.is_empty() && !comm.starts_with("pipewire") && comm != "wireplumber" && !names.contains(&comm) {
                names.push(comm);
            }
        }
        names.sort();
        names
    })
    .await
    .unwrap_or_default();
    if let Ok(mut c) = CAMERAS.lock() {
        *c = found;
    }
}

/// A kick on a change PipeWire reports to what the sound's state is made of: the sinks and sources, the cards
/// (their routes, the headset's profile), the default metadata, and their coming and going; the apps' streams
/// coming, going and starting or stopping (their state), not their every other change. pw-dump's monitor reports
/// every object's change, a browser's client and streams several times a second among them, and a kick costs a
/// pw-dump of the whole graph, so the rest is passed over. Its stream of JSON arrays is read on a thread
/// of its own, serde_json's streaming reader being a blocking one.
pub async fn events(kick: Kick) {
    let Ok(mut child) = tokio::process::Command::new("pw-dump")
        .args(["--monitor", "--no-colors"])
        .stdout(std::process::Stdio::piped())
        .spawn()
    else {
        return;
    };
    let Some(Ok(out)) = child.stdout.take().map(|o| o.into_owned_fd()) else { return };
    let _ = tokio::task::spawn_blocking(move || {
        let (mut watched, mut streams) = (HashSet::new(), HashSet::new());
        let reader = BufReader::new(std::fs::File::from(out));
        for objs in serde_json::Deserializer::from_reader(reader).into_iter::<Vec<Value>>() {
            let Ok(objs) = objs else { return };
            let mut touched = false;
            for o in &objs {
                let id = num(o, &["id"]);
                let info = o.get("info").filter(|i| !i.is_null());
                let kind = str(o, &["type"]);
                let props = info.and_then(|i| i.get("props")).filter(|p| !p.is_null());
                if info.is_none() && kind.is_empty() {
                    // gone
                    touched |= watched.remove(&id) | streams.remove(&id);
                    continue;
                }
                if kind == "PipeWire:Interface:Device"
                    || kind == "PipeWire:Interface:Metadata" && str(o, &["props", "metadata.name"]) == "default"
                {
                    watched.insert(id);
                } else if let (Some(p), "PipeWire:Interface:Node") = (props, kind) {
                    // a node says its class when its props change; streams and the rest are not watched
                    let class = str(p, &["media.class"]);
                    if matches!(class, "Audio/Sink" | "Audio/Source") {
                        watched.insert(id);
                    } else {
                        watched.remove(&id);
                    }
                    // a Video/Source among them for a screen's coming and going
                    let stream = matches!(class, "Stream/Input/Audio" | "Stream/Input/Video" | "Stream/Output/Audio");
                    if stream || class == "Video/Source" {
                        touched |= streams.insert(id);
                    }
                }
                let state = info
                    .and_then(|i| i["change-mask"].as_array())
                    .is_some_and(|m| m.iter().any(|c| c == "state"));
                touched |= watched.contains(&id) || state && streams.contains(&id);
            }
            if touched {
                STALE.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if touched && kick.send_blocking(()).is_err() {
                return;
            }
        }
    })
    .await;
    let _ = child.wait().await;
}

/// audio volume ID LEVEL: a stream's level (0 to 1, as wpctl puts it), through wpctl. The state is read anew
/// after: PipeWire's monitor tells a level's change to nothing watched.
pub async fn cmd(args: &[&str]) -> Res {
    // a card's profile switched (the speaker's, from the headphones'), then the port asked for put on in it:
    // WirePlumber restores the routes it saved for the profile, which may hold no output at all
    if let ["port", card, profile, route, device] = args {
        let n = |s: &str| s.parse::<u32>().map_err(|_| "audio port: CARD PROFILE ROUTE DEVICE".to_string());
        let (card, profile, route, device) = (n(card)?, n(profile)?, n(route)?, n(device)?);
        let run = |prog: &'static str, args: Vec<String>| async move {
            let status = tokio::process::Command::new(prog).args(&args).status().await.map_err(|e| e.to_string())?;
            if status.success() { Ok(()) } else { Err(format!("{prog}: {status}")) }
        };
        run("wpctl", vec!["set-profile".into(), card.to_string(), profile.to_string()]).await?;
        // the profile's nodes made before its route is set
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let param = format!("{{ index: {route}, device: {device}, save: true }}");
        run("pw-cli", vec!["set-param".into(), card.to_string(), "Route".into(), param]).await?;
        STALE.store(true, std::sync::atomic::Ordering::Relaxed);
        return Ok(());
    }
    let ["volume", id, v] = args else { return Err(super::MODULE.usage()) };
    let (Ok(id), Ok(v)) = (id.parse::<u32>(), v.parse::<f64>()) else { return Err("audio volume: ID LEVEL".into()) };
    let status = tokio::process::Command::new("wpctl")
        .args(["set-volume", &id.to_string(), &format!("{:.2}", v.clamp(0.0, 1.0))])
        .status()
        .await
        .map_err(|e| e.to_string())?;
    STALE.store(true, std::sync::atomic::Ordering::Relaxed);
    if status.success() { Ok(()) } else { Err(status.to_string()) }
}

/// Flips the Bluetooth headset between headphones (its best A2DP profile: the good sound, no mic) and handsfree
/// (its best headset-head-unit: with the mic), through wpctl.
pub async fn headset() -> Res {
    let objs = pw_dump().await?;
    let Some(o) = objs.iter().find(|o| {
        str(o, &["type"]) == "PipeWire:Interface:Device" && str(o, &["info", "props", "device.api"]) == "bluez5"
    }) else {
        return Err("no Bluetooth headset".into());
    };
    let mut want = "headset-head-unit";
    if str(param(o, "Profile"), &["name"]).starts_with(want) {
        want = "a2dp-sink";
    }
    let (mut best, mut prio) = (-1, -1);
    for e in params(o, "EnumProfile") {
        if str(e, &["name"]).starts_with(want) && num(e, &["priority"]) > prio {
            (best, prio) = (num(e, &["index"]), num(e, &["priority"]));
        }
    }
    if best < 0 {
        return Err(format!("the headset has no {want} profile"));
    }
    let status = tokio::process::Command::new("wpctl")
        .args(["set-profile", &num(o, &["id"]).to_string(), &best.to_string()])
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(status.to_string()) }
}

/// The default output (sink) or input (source) as wpctl names it.
fn default(sink: bool) -> &'static str {
    if sink { "@DEFAULT_AUDIO_SINK@" } else { "@DEFAULT_AUDIO_SOURCE@" }
}

/// wpctl ARGS run to its end, blocking: the keys (keys.rs) want the level it leaves at once, before PipeWire's
/// monitor has the state read again.
fn wpctl(args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("wpctl").args(args).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// wpctl get-volume's "Volume: 0.45 [MUTED]" as a level 0 to 100 and whether muted.
fn parse_volume(s: &str) -> Option<(i32, bool)> {
    let v: f64 = s.split_whitespace().nth(1)?.parse().ok()?;
    Some(((v * 100.0).round() as i32, s.contains("[MUTED]")))
}

/// The default sink's or source's level 0 to 100 and whether muted.
pub fn volume(sink: bool) -> Result<(i32, bool), String> {
    let out = wpctl(&["get-volume", default(sink)])?;
    parse_volume(&out).ok_or_else(|| format!("wpctl: {}", out.trim()))
}

/// The default sink's or source's volume changed by a step as wpctl takes it (2%+, 2%-), never past 100%; then
/// its level and whether muted.
pub fn step(sink: bool, by: &str) -> Result<(i32, bool), String> {
    wpctl(&["set-volume", "-l", "1.0", default(sink), by])?;
    volume(sink)
}

/// The default sink's or source's mute flipped; then its level and whether muted.
pub fn mute(sink: bool) -> Result<(i32, bool), String> {
    wpctl(&["set-mute", default(sink), "toggle"])?;
    volume(sink)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse() {
        let objs: Vec<serde_json::Value> = serde_json::from_str(include_str!("audio_test.json")).unwrap();
        let v = serde_json::to_value(super::parse(&objs)).unwrap();
        assert_eq!(v["sinks"], serde_json::json!([{"id": 40, "name": "Speakers", "def": true}]));
        assert_eq!(v["volume"], serde_json::json!(0.5));
        assert_eq!(v["micApps"], serde_json::json!([{"id": 88, "name": "Chromium input"}]));
        assert_eq!(v["camApps"], serde_json::json!(["zoom"]));
        assert_eq!(v["screen"], serde_json::json!(true));
        assert_eq!(v["screenApps"], serde_json::json!(["obs"]));
        assert_eq!(
            v["streams"],
            serde_json::json!([
                {"id": 116, "name": "Chromium", "icon": "", "bin": "chromium", "volume": 1},
                {"id": 120, "name": "mpv", "icon": "mpv", "bin": "mpv", "volume": 0.5},
            ])
        );
    }

    #[test]
    fn a_card_output_in_another_profile_is_a_switch() {
        let card = serde_json::json!({"type": "PipeWire:Interface:Device", "id": 48, "info": {
            "props": {"device.api": "alsa"},
            "params": {"Profile": [{"index": 1}], "EnumRoute": [
                {"direction": "Output", "description": "Headphones", "available": "yes", "profiles": [1]},
                {"index": 1, "direction": "Output", "description": "Speaker", "available": "unknown", "profiles": [2], "devices": [3]},
                {"direction": "Output", "description": "HDMI", "available": "no", "profiles": [3]}]}}});
        let v = serde_json::to_value(super::parse(&[card])).unwrap();
        assert_eq!(v["sinks"], serde_json::json!([{"id": 0, "name": "Speaker", "def": false, "card": 48, "profile": 2, "route": [1, 3]}]));
    }

    #[test]
    fn parse_volume() {
        assert_eq!(super::parse_volume("Volume: 0.45\n"), Some((45, false)));
        assert_eq!(super::parse_volume("Volume: 1.00 [MUTED]\n"), Some((100, true)));
        assert_eq!(super::parse_volume("nothing"), None);
    }

    #[tokio::test]
    async fn audio_state() {
        println!("audio {}", super::state().await);
    }
}
