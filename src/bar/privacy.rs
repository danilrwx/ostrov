//! What records: a mic while an app takes it, a camera while one is in use (audio.rs's micApps and camApps), in
//! the recording colour, the apps in its tooltip. Nothing, not even its padding, while nothing records.

use std::rc::Rc;

use gtk4::prelude::*;

use super::{pill, slot, Block, Ctx};

pub fn build(cx: &Rc<Ctx>) -> Block {
    let p = pill();
    p.add_css_class("recording");
    let mic = gtk4::Image::from_icon_name("audio-input-microphone-symbolic");
    let cam = gtk4::Image::from_icon_name("camera-web-symbolic");
    p.append(&mic);
    p.append(&cam);
    p.set_visible(false);
    // the slot kept, empty, so the bar's middle still ends at it
    let s = slot(&p);
    cx.hub.on(move |st| {
        let a = &st["audio"];
        let names = |list: &str, name: fn(&serde_json::Value) -> &str| -> Vec<String> {
            let mut v: Vec<String> = a[list].as_array().into_iter().flatten().map(|x| name(x).to_string()).collect();
            v.sort();
            v.dedup();
            v
        };
        let mics = names("micApps", |x| x["name"].as_str().unwrap_or(""));
        let cams = names("camApps", |x| x.as_str().unwrap_or(""));
        mic.set_visible(!mics.is_empty());
        cam.set_visible(!cams.is_empty());
        p.set_visible(!mics.is_empty() || !cams.is_empty());
        let mut tip = Vec::new();
        if !mics.is_empty() {
            tip.push(crate::i18n::fill(crate::i18n::t("Microphone: {}"), &[&mics.join(", ")]));
        }
        if !cams.is_empty() {
            tip.push(crate::i18n::fill(crate::i18n::t("Camera: {}"), &[&cams.join(", ")]));
        }
        p.set_tooltip_text(Some(&tip.join("\n")));
    });
    Block::new(&s)
}
