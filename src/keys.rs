//! The media keys and a laptop's Fn keys: `ostrov key NAME` from the compositor's binds. The volume and the mic through wpctl, the backlight through logind, the
//! player through MPRIS, the touchpad through Hyprland's per-device setting; what each did shown in the OSD
//! (notes.rs), its level the new one. The performance and camera keys, on a laptop whose vendor
//! service acts on them (HONOR's honor-hotkey-actions), show only what it did. And the battery's warnings as it runs down (battery).

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::{gio, glib};

use crate::hub::Hub;
use crate::notes::Notes;
use crate::modules::{audio::service as audio, brightness::service as backlight};

/// A level's step, in percent, as the script's.
const STEP: i64 = 2;
/// How long the OSD stays: a level a second, a word a second and a half.
const LEVEL_MS: u64 = 1000;
const WORD_MS: u64 = 1500;

/// The keys' names, `ostrov key NAME`'s.
pub const NAMES: &[&str] = &[
    "vol-up", "vol-down", "vol-mute", "mic-up", "mic-down", "mic", "bright-up", "bright-down", "touchpad-on",
    "touchpad-off", "touchpad-toggle", "profile", "camera", "play-pause", "next", "previous",
];

pub struct Keys {
    hub: Rc<Hub>,
    notes: Rc<Notes>,
    /// the touchpad's state, as Hyprland does not report it; on as Hyprland starts
    touchpad: Cell<bool>,
    /// the last toggle, a second one within half a second dropped (the key arriving twice)
    toggled: Cell<Option<Instant>>,
    /// the brightness last asked for and when: a held key's next step goes from it, the backlight's own reading
    /// lagging logind's call
    bright: Cell<Option<(Instant, i64)>>,
}

/// The speaker's or the mic's icon by how loud, muted at 0.
fn icon(sink: bool, level: i32) -> String {
    let kind = if sink { "audio-volume" } else { "microphone-sensitivity" };
    let how = match level {
        ..=0 => "muted",
        1..34 => "low",
        34..67 => "medium",
        _ => "high",
    };
    format!("{kind}-{how}-symbolic")
}

impl Keys {
    pub fn new(hub: &Rc<Hub>, notes: &Rc<Notes>) -> Rc<Keys> {
        Rc::new(Keys {
            hub: hub.clone(),
            notes: notes.clone(),
            touchpad: Cell::new(true),
            toggled: Cell::new(None),
            bright: Cell::new(None),
        })
    }

    /// ostrov key NAME.
    pub fn key(self: &Rc<Self>, name: &str) -> Result<(), String> {
        let up = format!("{STEP}%+");
        let down = format!("{STEP}%-");
        match name {
            "vol-up" => self.volume(true, Some(up)),
            "vol-down" => self.volume(true, Some(down)),
            "vol-mute" => self.volume(true, None),
            "mic-up" => self.volume(false, Some(up)),
            "mic-down" => self.volume(false, Some(down)),
            "mic" => self.volume(false, None),
            "bright-up" => return self.brightness(STEP),
            "bright-down" => return self.brightness(-STEP),
            "touchpad-on" => return self.touchpad(Some(true)),
            "touchpad-off" => return self.touchpad(Some(false)),
            "touchpad-toggle" => return self.touchpad(None),
            "profile" => self.later(600, |k| {
                let st = k.hub.state();
                let p = crate::hub::s(&st, &["power", "active"]);
                k.notes.osd(&format!("power-profile-{p}-symbolic"), &format!("Power profile  {p}"), None, WORD_MS);
            }),
            "camera" => self.later(500, |k| {
                let on = std::path::Path::new("/dev/video0").exists();
                let (icon, word) = if on { ("camera-web-symbolic", "on") } else { ("camera-disabled-symbolic", "off") };
                k.notes.osd(icon, &format!("Camera  {word}"), None, WORD_MS);
            }),
            "play-pause" | "next" | "previous" => crate::hub::service(&["media", name]),
            _ => return Err(crate::forms::usage("key", &[&NAMES.join("|")])),
        }
        Ok(())
    }

    /// f after ms: what the HONOR's service did, given the time to do it.
    fn later(self: &Rc<Self>, ms: u64, f: impl FnOnce(&Keys) + 'static) {
        let k = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(ms), move || f(&k));
    }

    /// The speaker's (sink) or the mic's volume stepped (by, as wpctl takes it), or its mute flipped (None); wpctl
    /// off GTK's thread, the OSD shown with the level it leaves.
    fn volume(self: &Rc<Self>, sink: bool, by: Option<String>) {
        let k = self.clone();
        glib::spawn_future_local(async move {
            let stepped = by.is_some();
            let r = gio::spawn_blocking(move || match by {
                Some(by) => audio::step(sink, &by),
                None => audio::mute(sink),
            })
            .await;
            let (level, muted) = match r {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => return eprintln!("ostrov: key: wpctl: {e}"),
                Err(_) => return,
            };
            let level = if muted { 0 } else { level };
            let what = if sink { "Volume" } else { "Microphone" };
            match (stepped, muted) {
                (true, _) => k.notes.osd(&icon(sink, level), &format!("{level}%"), Some(level), LEVEL_MS),
                (false, true) => k.notes.osd(&icon(sink, 0), &format!("{what}  muted"), None, WORD_MS),
                (false, false) => k.notes.osd(&icon(sink, level), &format!("{what}  on"), Some(level), WORD_MS),
            }
        });
    }

    /// The backlight a step up or down, from the level last asked for while a held key repeats.
    fn brightness(&self, by: i64) -> Result<(), String> {
        let recent = self.bright.get().filter(|(t, _)| t.elapsed() < Duration::from_millis(500)).map(|(_, p)| p);
        let now = match recent {
            Some(p) => p,
            None => match backlight::brightness() {
                -1 => return Err("no backlight".into()),
                p => p,
            },
        };
        let pct = (now + by).clamp(0, 100);
        self.bright.set(Some((Instant::now(), pct)));
        crate::hub::service(&["brightness", &pct.to_string()]);
        self.notes.osd("display-brightness-symbolic", &format!("{pct}%"), Some(pct as i32), LEVEL_MS);
        Ok(())
    }

    /// The touchpad on, off, or flipped (None), through Hyprland's setting for it by its name.
    fn touchpad(&self, on: Option<bool>) -> Result<(), String> {
        if crate::wm::wm() != crate::wm::Wm::Hyprland {
            return Err("the touchpad's key is Hyprland's".into());
        }
        if on.is_none() {
            if self.toggled.get().is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
                return Ok(());
            }
            self.toggled.set(Some(Instant::now()));
        }
        let devices: serde_json::Value = serde_json::from_str(&crate::wm::hyprctl("j/devices")).unwrap_or_default();
        let pad = devices["mice"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["name"].as_str())
            .find(|n| n.to_lowercase().contains("touchpad"))
            .ok_or("no touchpad")?;
        let on = on.unwrap_or(!self.touchpad.get());
        self.touchpad.set(on);
        crate::wm::hyprctl(&format!("keyword device[{pad}]:enabled {on}"));
        let (icon, word) = if on { ("input-touchpad-symbolic", "on") } else { ("touchpad-disabled-symbolic", "off") };
        self.notes.osd(icon, &format!("Touchpad  {word}"), None, WORD_MS);
        Ok(())
    }
}

/// Battery warnings: discharging, a notification at 20% and at 10%, and at 5% a critical one with the question
/// whether to suspend; each once a discharge, all of them again once it charges.
pub fn battery(hub: &Rc<Hub>, notes: &Rc<Notes>, prompts: &Rc<crate::prompt::Prompts>) {
    // the last level warned of, 100 for none yet
    let warned = Cell::new(100u8);
    let (notes, prompts) = (notes.clone(), prompts.clone());
    hub.on(move |st| {
        let b = &st["battery"];
        if !b["present"].as_bool().unwrap_or(false) {
            return;
        }
        let pct = b["percent"].as_f64().unwrap_or(100.0);
        match crate::hub::s(b, &["state"]) {
            "charging" | "fully-charged" | "pending-charge" => warned.set(100),
            "discharging" => {
                let Some(level) = due(warned.get(), pct) else { return };
                warned.set(level);
                let left = crate::ui::battery_time(b);
                let pct = pct.round();
                let body = if left.is_empty() { format!("{pct}%") } else { format!("{pct}%, {left}") };
                if level > 5 {
                    notes.post("battery-low-symbolic", "Battery low", &body, false);
                    return;
                }
                notes.post("battery-caution-symbolic", "Battery critical", &body, true);
                let (reply, answer) = async_channel::bounded(1);
                let text = format!("{body}. Suspend now?");
                let kind = crate::prompt::Kind::Confirm;
                let icon = "battery-caution-symbolic";
                prompts.ask(crate::prompt::Ask::new(icon, "Battery critical", &text, kind, reply));
                glib::spawn_future_local(async move {
                    if let Ok(Some(_)) = answer.recv().await {
                        crate::hub::run(&["systemctl", "suspend"]);
                    }
                });
            }
            _ => {}
        }
    });
}

/// The warning due at pct, warned the last level warned of (100 for none): the lowest level under that one pct
/// has come down to, so a start already at 8% warns of 10% alone.
fn due(warned: u8, pct: f64) -> Option<u8> {
    [20u8, 10, 5].into_iter().filter(|&l| l < warned && pct <= f64::from(l)).last()
}

#[cfg(test)]
mod tests {
    #[test]
    fn due() {
        assert_eq!(super::due(100, 50.0), None);
        assert_eq!(super::due(100, 20.0), Some(20));
        assert_eq!(super::due(20, 19.0), None);
        assert_eq!(super::due(20, 10.0), Some(10));
        assert_eq!(super::due(100, 8.0), Some(10));
        assert_eq!(super::due(10, 4.0), Some(5));
        assert_eq!(super::due(5, 1.0), None);
        assert_eq!(super::due(100, 3.0), Some(5));
    }

    #[test]
    fn icon() {
        assert_eq!(super::icon(true, 0), "audio-volume-muted-symbolic");
        assert_eq!(super::icon(true, 33), "audio-volume-low-symbolic");
        assert_eq!(super::icon(false, 50), "microphone-sensitivity-medium-symbolic");
        assert_eq!(super::icon(true, 100), "audio-volume-high-symbolic");
    }
}
