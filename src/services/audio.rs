//! The sound devices through pw-dump, the headset's profile through wpctl (audio.go).

use std::collections::{HashMap, HashSet};
use std::io::BufReader;

use serde::Serialize;
use serde_json::Value;

use super::{Kick, Res};

/// A PipeWire sink or source: its node id (for wpctl set-default), its name, whether it is the default.
#[derive(Serialize)]
struct Sound {
    id: i64,
    name: String,
    def: bool,
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
    let mut a = Audio::default();
    let objs = pw_dump().await?;
    let mut defaults = HashMap::new();
    let mut routes = HashMap::new();
    for o in &objs {
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
    for o in &objs {
        if str(o, &["type"]) != "PipeWire:Interface:Node" {
            continue;
        }
        let p = &o["info"]["props"];
        let (sink, key) = match str(p, &["media.class"]) {
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
        list.push(Sound { id: num(o, &["id"]), name: name.into(), def });
        // the default's level: wpctl's is the cube root of the first channel's volume
        let pr = param(o, "Props");
        if def && !pr.is_null() {
            let level = pr["channelVolumes"][0].as_f64().map_or(0.0, |v| (v.cbrt() * 100.0).round() / 100.0);
            let mute = pr["mute"].as_bool().unwrap_or(false);
            if sink {
                (a.volume, a.muted) = (level, mute);
            } else {
                (a.mic, a.mic_muted) = (level, mute);
            }
        }
    }
    a.sinks.sort_by(|x, y| x.name.cmp(&y.name));
    a.sources.sort_by(|x, y| x.name.cmp(&y.name));
    Ok(a)
}

/// The sound's state last read, and whether PipeWire has changed it since: every state the watcher sends (every
/// 3 s besides a change) would run a pw-dump of the whole graph otherwise, and the monitor (events) tells every
/// change that counts.
static LAST: std::sync::Mutex<Value> = std::sync::Mutex::new(Value::Null);
static STALE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub async fn state() -> Value {
    if !STALE.swap(false, std::sync::atomic::Ordering::Relaxed) {
        return LAST.lock().map(|v| v.clone()).unwrap_or_default();
    }
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
}

/// A kick on a change PipeWire reports to what the sound's state is made of: the sinks and sources, the cards
/// (their routes, the headset's profile), the default metadata, and their coming and going. pw-dump's monitor
/// reports every object's change, a browser's client and streams several times a second among them, and a kick
/// costs a pw-dump of the whole graph, so the rest is passed over. Its stream of JSON arrays is read on a thread
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
        let mut watched = HashSet::new();
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
                    touched |= watched.remove(&id);
                    continue;
                }
                if kind == "PipeWire:Interface:Device"
                    || kind == "PipeWire:Interface:Metadata" && str(o, &["props", "metadata.name"]) == "default"
                {
                    watched.insert(id);
                } else if let (Some(p), "PipeWire:Interface:Node") = (props, kind) {
                    // a node says its class when its props change; streams and the rest are not watched
                    if matches!(str(p, &["media.class"]), "Audio/Sink" | "Audio/Source") {
                        watched.insert(id);
                    } else {
                        watched.remove(&id);
                    }
                }
                touched |= watched.contains(&id);
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

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn audio_state() {
        println!("audio {}", super::state().await);
    }
}
