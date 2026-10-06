//! ostrov's screen recorder, over wf-recorder: `ostrov plugin record toggle [--audio]` (SUPER SHIFT+R) asks for a
//! region with ostrov's own selector (`ostrov pick-region`, a click alone the whole screen, Escape nothing) and
//! records it to ~/Videos/Recordings/DATE_TIME.mp4 until the same command again, which interrupts wf-recorder as
//! Ctrl+C would, so it finishes the file; --audio adds what the default sink plays. Done, the file's path is on
//! the clipboard and in a toast. Its widget's badge, a red dot and the time gone by, is in the bar while recording.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use ostrov_plugin::{Host, Plugin, Value, json, t};

/// The timer a recording's thread wakes the plugin with, once it has sent what happened.
const WAKE: u32 = 1;
/// The badge's clock, a second at a time while recording.
const TICK: u32 = 2;

/// What a recording's thread saw.
enum Found {
    /// wf-recorder started: its pid, its file
    Started(u32, PathBuf),
    /// nothing picked, nothing recorded
    Cancelled,
    /// wf-recorder ended: its file, or what went wrong
    Ended(Result<PathBuf, String>),
}

struct Record {
    /// while recording: wf-recorder's pid, since when
    on: Option<(u32, Instant)>,
    /// a region being picked, or wf-recorder starting
    starting: bool,
    found: (Sender<Found>, Receiver<Found>),
}

impl Record {
    fn toggle(&mut self, host: &Host, audio: bool) -> Result<String, String> {
        if let Some((pid, _)) = self.on {
            // wf-recorder finishes the file on SIGINT; its thread sees it end
            Command::new("kill").args(["-INT", &pid.to_string()]).status().map_err(|e| format!("kill: {e}"))?;
            return Ok("stopping".into());
        }
        if self.starting {
            return Ok("starting".into());
        }
        self.starting = true;
        let (h, tx) = (host.clone(), self.found.0.clone());
        // the region waits for the user, wf-recorder for its end: a thread of their own
        std::thread::spawn(move || {
            let send = |f| {
                let _ = tx.send(f);
                h.set_timer(0, WAKE);
            };
            let Ok(region) = h.run(&["pick-region"]) else { return send(Found::Cancelled) };
            // the region framed on the screen while it is recorded (outside it, not in the video)
            let _ = h.run(&["outline", region.trim()]);
            let ended = record(region.trim(), audio, |pid, file| send(Found::Started(pid, file.into())));
            let _ = h.run(&["outline", "off"]);
            send(ended);
        });
        Ok("starting".into())
    }
}

/// A recording of the region, waited for: wf-recorder started (`started` told its pid) and ended.
fn record(region: &str, audio: bool, started: impl Fn(u32, &Path)) -> Found {
    let dir = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Videos/Recordings");
    let stamp = Command::new("date").arg("+%Y-%m-%d_%H-%M-%S").output().ok();
    let stamp = stamp.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty());
    let file = dir.join(format!("{}.mp4", stamp.as_deref().unwrap_or("recording")));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Found::Ended(Err(format!("{}: {e}", dir.display())));
    }
    let child = Command::new("wf-recorder")
        .args(args(region, &file, audio))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let child = match child {
        Ok(c) => c,
        Err(e) => return Found::Ended(Err(format!("wf-recorder: {e}"))),
    };
    started(child.id(), &file);
    Found::Ended(match child.wait_with_output() {
        Ok(o) if o.status.success() => Ok(file),
        // its last line says why
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).lines().rev().find(|l| !l.trim().is_empty())
            .map_or_else(|| format!("wf-recorder: {}", o.status), |l| format!("wf-recorder: {}", l.trim()))),
        Err(e) => Err(format!("wf-recorder: {e}")),
    })
}

/// wf-recorder's arguments: the region (slurp's "X,Y WxH"), the file, never asking to overwrite; with audio,
/// the default sink's monitor.
fn args(region: &str, file: &Path, audio: bool) -> Vec<String> {
    let mut a = vec!["-y".into(), "-g".into(), region.into(), "-f".into(), file.display().to_string()];
    if audio {
        a.push("--audio=@DEFAULT_MONITOR@".into());
    }
    a
}

/// Time gone by, as m:ss, or h:mm:ss past an hour.
fn elapsed(s: u64) -> String {
    match s / 3600 {
        0 => format!("{}:{:02}", s / 60, s % 60),
        hr => format!("{hr}:{:02}:{:02}", s / 60 % 60, s % 60),
    }
}

impl Plugin for Record {
    fn state(&mut self, _: &Host) -> Value {
        json!({"recording": self.on.is_some()})
    }

    fn run(&mut self, host: &Host, args: &[&str], _: Option<&str>) -> Result<String, String> {
        match args {
            ["toggle"] => self.toggle(host, false),
            ["toggle", "--audio"] | ["--audio"] => self.toggle(host, true),
            _ => Err(format!("no command {:?}", args.join(" "))),
        }
    }

    fn on_timer(&mut self, host: &Host, id: u32) {
        if id == TICK && self.on.is_some() {
            host.set_timer(1000, TICK);
        }
        while let Ok(found) = self.found.1.try_recv() {
            match found {
                Found::Started(pid, file) => {
                    host.log(format!("recording to {}", file.display()));
                    self.on = Some((pid, Instant::now()));
                    host.set_timer(1000, TICK);
                }
                Found::Cancelled => {}
                Found::Ended(r) => {
                    self.on = None;
                    let (title, body) = match r {
                        Ok(file) => {
                            let path = file.display().to_string();
                            if let Err(e) = Command::new("wl-copy").arg(&path).status() {
                                host.log(format!("wl-copy: {e}"));
                            }
                            (t("Recording saved"), path)
                        }
                        Err(e) => {
                            host.log(&e);
                            (t("Recording failed"), e)
                        }
                    };
                    let _ = host.toast(title, &body);
                }
            }
            self.starting = false;
        }
        host.kick();
    }

    fn render(&mut self, _: &Host, widget: &str) -> Option<Value> {
        let time = self.on.map(|(_, since)| elapsed(since.elapsed().as_secs()));
        if widget == "record#bar" {
            // standing in the bar by itself (widget.plugin.record.record), a click there starts or stops it
            return Some(json!({"type": "box", "orientation": "horizontal", "active": time.is_some(), "click": "toggle",
                "children": [
                {"type": "label", "text": "●", "class": "error"},
                {"type": "label", "text": time.clone().unwrap_or_default()},
            ]}));
        }
        Some(json!({
            "type": "toggle", "id": "toggle", "icon": "media-record-symbolic", "title": t("Screen Recording"),
            "sub": time.unwrap_or_else(|| t("Off").into()), "on": self.on.is_some(),
            "menu": {"type": "box", "children": [
                if self.on.is_some() {
                    json!({"type": "row", "id": "toggle", "icon": "media-playback-stop-symbolic", "text": t("Stop")})
                } else {
                    let text = t("Record with sound");
                    json!({"type": "row", "id": "audio", "icon": "audio-speakers-symbolic", "text": text})
                },
            ]},
        }))
    }

    fn on_event(&mut self, host: &Host, _: &str, node: &str, _: &str, _: &str) {
        let r = match node {
            "toggle" => self.toggle(host, false),
            "audio" => self.toggle(host, true),
            _ => return,
        };
        if let Err(e) = r {
            host.log(e);
        }
        // the toggle drawn as it is, not as clicked
        host.kick();
    }
}

fn main() {
    ostrov_plugin::run(Record { on: None, starting: false, found: channel() });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_built() {
        let file = Path::new("/home/u/My Videos/a.mp4");
        assert_eq!(args("10,20 300x200", file, false), ["-y", "-g", "10,20 300x200", "-f", "/home/u/My Videos/a.mp4"]);
        assert_eq!(args("0,0 1x1", file, true).last().map(String::as_str), Some("--audio=@DEFAULT_MONITOR@"));
    }

    #[test]
    fn elapsed_said() {
        assert_eq!(elapsed(7), "0:07");
        assert_eq!(elapsed(605), "10:05");
        assert_eq!(elapsed(3725), "1:02:05");
    }
}
