use std::cell::RefCell;

use gtk4::prelude::*;
use gtk4::Orientation;
use serde_json::Value;

use crate::cc::{Ctx, Face, Widget};
use crate::hub::{run, s, service};
use crate::style::{clear, label};
use crate::ui::{app_icon, arrow, level_icon, menu, row, Memo, Slider, Toggle};

/// The volume: the default output's, mute on its icon; its menu the outputs, then the apps playing, a slider
/// each.
pub fn volume(c: &Ctx) -> Widget {
    let vol = Slider::new("audio-volume-high-symbolic", |v| run(&["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{v:.2}")]));
    vol.icon.connect_clicked(|_| run(&["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]));
    vol.root.append(&arrow(c.flip.clone()));
    let (card, items) = menu("audio-speakers-symbolic", "Sound Output");
    let outs = gtk4::Box::new(Orientation::Vertical, 0);
    let apps = gtk4::Box::new(Orientation::Vertical, 4);
    items.append(&outs);
    items.append(&apps);
    let sliders: RefCell<Vec<Slider>> = RefCell::default();
    let memo = Memo::default();
    let root = vol.root.clone();
    let face = Face::new(&vol.face);
    let w = Widget::new(&root, Some(&card), move |st| {
        let a = &st["audio"];
        let (v, m) = (a["volume"].as_f64().unwrap_or(0.0), a["muted"].as_bool().unwrap_or(false));
        vol.set(v, &level_icon("audio-volume", v, m));
        devices(&memo, "outs", &a["sinks"], &outs);
        let streams = a["streams"].as_array().cloned().unwrap_or_default();
        let who = |x: &Value| format!("{} {} {} {}", x["id"], x["name"], x["icon"], x["bin"]);
        if memo.changed("apps", streams.iter().map(who).collect()) {
            clear(&apps);
            let mut sliders = sliders.borrow_mut();
            sliders.clear();
            if !streams.is_empty() {
                apps.append(&gtk4::Separator::new(Orientation::Horizontal));
            }
            for x in &streams {
                let id = x["id"].to_string();
                let sl = Slider::new("", move |v| service(&["audio", "volume", &id, &format!("{v:.2}")]));
                sl.icon.set_child(Some(&app_icon(s(x, &["icon"]), s(x, &["bin"]), s(x, &["name"]))));
                sl.icon.set_can_target(false);
                let name = label(s(x, &["name"]), "dim");
                name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                apps.append(&name);
                apps.append(&sl.root);
                sliders.push(sl);
            }
        }
        for (sl, x) in sliders.borrow().iter().zip(&streams) {
            sl.set(x["volume"].as_f64().unwrap_or(0.0), "");
        }
    });
    Widget { face: Some(face), ..w }
}

/// A list of sound devices, the default ticked, a click making one the default.
fn devices(memo: &Memo, key: &'static str, list: &Value, items: &gtk4::Box) {
    if !memo.changed(key, list.to_string()) {
        return;
    }
    clear(items);
    for d in list.as_array().into_iter().flatten() {
        let id = d["id"].to_string();
        items.append(&row("", s(d, &["name"]), "", d["def"].as_bool().unwrap_or(false), move || {
            run(&["wpctl", "set-default", &id])
        }));
    }
}

pub fn mic(c: &Ctx) -> Widget {
    let mic = Slider::new("microphone-sensitivity-high-symbolic", |v| run(&["wpctl", "set-volume", "@DEFAULT_AUDIO_SOURCE@", &format!("{v:.2}")]));
    mic.icon.connect_clicked(|_| run(&["wpctl", "set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"]));
    mic.root.append(&arrow(c.flip.clone()));
    let (card, items) = menu("audio-input-microphone-symbolic", "Sound Input");
    let memo = Memo::default();
    let root = mic.root.clone();
    let face = Face::new(&mic.face);
    let w = Widget::new(&root, Some(&card), move |st| {
        let a = &st["audio"];
        let (v, m) = (a["mic"].as_f64().unwrap_or(0.0), a["micMuted"].as_bool().unwrap_or(false));
        mic.set(v, &level_icon("microphone-sensitivity", v, m));
        devices(&memo, "ins", &a["sources"], &items);
    });
    Widget { face: Some(face), ..w }
}

/// The headset's mode, handsfree (with its mic) or headphones; there only while a headset is.
pub fn headset(_: &Ctx) -> Widget {
    let t = Toggle::new("audio-headphones-symbolic", "Headset", || service(&["headset"]), None);
    let t2 = t.clone();
    Widget::toggle(&t, None, move |st| {
        let hs = s(&st["audio"], &["headset"]);
        t2.root.set_visible(!hs.is_empty());
        t2.set(hs == "handsfree", "", if hs == "handsfree" { "Handsfree, with the mic" } else { "Headphones" });
    })
}
