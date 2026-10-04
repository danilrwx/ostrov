//! What records: a mic while an app takes it, a camera while one is in use (audio.rs's micApps and camApps), the
//! screen while shared (Hyprland's screencast event, or a portal's stream in PipeWire: audio.rs's screen), in the
//! recording colour, the apps in its tooltip. Nothing, not even its padding, while nothing records.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::{pill, slot, Block, Ctx};
use crate::i18n::{fill, t};
use crate::wm::Event;

pub fn build(cx: &Rc<Ctx>) -> Block {
    let p = pill();
    p.add_css_class("recording");
    let mic = gtk4::Image::from_icon_name("audio-input-microphone-symbolic");
    let cam = gtk4::Image::from_icon_name("camera-web-symbolic");
    let screen = gtk4::Image::from_icon_name("screen-shared-symbolic");
    p.append(&mic);
    p.append(&cam);
    p.append(&screen);
    p.set_visible(false);
    // the slot kept, empty, so the bar's middle still ends at it
    let s = slot(&p);
    // the screen shared as Hyprland last said
    let cast = Rc::new(Cell::new(false));
    let c = cast.clone();
    let draw = Rc::new(move |st: &serde_json::Value| {
        let a = &st["audio"];
        let names = |list: &str, name: fn(&serde_json::Value) -> &str| -> Vec<String> {
            let mut v: Vec<String> = a[list].as_array().into_iter().flatten().map(|x| name(x).to_string()).collect();
            v.sort();
            v.dedup();
            v
        };
        let mics = names("micApps", |x| x["name"].as_str().unwrap_or(""));
        let cams = names("camApps", |x| x.as_str().unwrap_or(""));
        let shares = names("screenApps", |x| x.as_str().unwrap_or(""));
        let shared = c.get() || a["screen"] == true || !shares.is_empty();
        mic.set_visible(!mics.is_empty());
        cam.set_visible(!cams.is_empty());
        screen.set_visible(shared);
        p.set_visible(!mics.is_empty() || !cams.is_empty() || shared);
        let mut tip = Vec::new();
        if !mics.is_empty() {
            tip.push(fill(t("Microphone: {}"), &[&mics.join(", ")]));
        }
        if !cams.is_empty() {
            tip.push(fill(t("Camera: {}"), &[&cams.join(", ")]));
        }
        if shared {
            tip.push(match shares.is_empty() {
                true => t("Screen shared").to_string(),
                false => fill(t("Screen shared: {}"), &[&shares.join(", ")]),
            });
        }
        p.set_tooltip_text(Some(&tip.join("\n")));
    });
    let d = draw.clone();
    cx.hub.on(move |st| d(st));
    let hub = cx.hub.clone();
    cx.on_wm(move |e| {
        if let Event::Screencast(on, _) = e {
            cast.set(*on);
            draw(&hub.state());
        }
    });
    Block::new(&s)
}
