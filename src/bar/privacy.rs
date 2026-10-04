//! What records: a mic while an app takes it, a camera while one is in use (audio.rs's micApps and camApps), the
//! screen while shared (Hyprland's screencast event, or a portal's stream in PipeWire: audio.rs's screen), in the
//! recording colour, the apps in its tooltip. Nothing, not even its padding, while nothing records. A click
//! unrolls a menu of what is in use, each with its way out: the mic muted, an app's camera turned off, the screen's
//! share stopped (`ostrov audio mic-mute | stop-camera APP | stop-screen`). `ostrov privacy` opens or closes it.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::{pill, slot, Block, Ctx};
use crate::hub::service;
use crate::i18n::{fill, t};
use crate::popup::{Popup, Side};
use crate::ui::{menu, row, Memo};
use crate::wm::Event;

/// A row that only says something: an app in use.
fn note(text: &str) -> gtk4::Button {
    let r = row("", text, "", false, || {});
    r.set_can_target(false);
    r
}

/// The menu: a card for the mic, the camera, the screen, each in use, its apps and its action. The screen's share
/// is stopped, not changed to another screen or window: the portal's session is the app's, the app asked again.
fn cards(
    body: &gtk4::Box,
    mics: &[String],
    muted: bool,
    cams: &[String],
    stoppable: &[String],
    shares: Option<&[String]>,
) {
    crate::style::clear(body);
    if !mics.is_empty() {
        let (card, items) = menu("audio-input-microphone-symbolic", t("Microphone"));
        mics.iter().for_each(|m| items.append(&note(m)));
        let (icon, text) = match muted {
            true => ("microphone-sensitivity-high-symbolic", t("Unmute microphone")),
            false => ("microphone-sensitivity-muted-symbolic", t("Mute microphone")),
        };
        items.append(&row(icon, text, "", false, || service(&["audio", "mic-mute"])));
        body.append(&card);
    }
    if !cams.is_empty() {
        let (card, items) = menu("camera-web-symbolic", t("Camera"));
        cams.iter().for_each(|c| items.append(&note(c)));
        // a process holding the camera itself, not through PipeWire, is not to be stopped from here
        for c in cams.iter().filter(|c| stoppable.contains(c)) {
            let app = c.clone();
            let off = row("camera-disabled-symbolic", &fill(t("Turn off for {}"), &[c]), "", false, move || {
                service(&["audio", "stop-camera", &app])
            });
            items.append(&off);
        }
        body.append(&card);
    }
    if let Some(shares) = shares {
        let (card, items) = menu("screen-shared-symbolic", t("Screen"));
        shares.iter().for_each(|s| items.append(&note(s)));
        items.append(&row("media-playback-stop-symbolic", t("Stop sharing"), "", false, || {
            service(&["audio", "stop-screen"])
        }));
        body.append(&card);
    }
}

pub fn build(cx: &Rc<Ctx>, side: Side) -> Block {
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
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    body.add_css_class("surface");
    let pop = Popup::new(&cx.host, &s, side, 300, &body);
    let click = gtk4::GestureClick::new();
    let pp = pop.clone();
    click.connect_released(move |_, _, _, _| pp.toggle());
    p.add_controller(click);
    p.set_cursor_from_name(Some("pointer"));
    let memo = Memo::default();
    let pp = pop.clone();
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
        let stoppable = names("camStreams", |x| x["name"].as_str().unwrap_or(""));
        let muted = a["micMuted"].as_bool().unwrap_or(false);
        let on = !mics.is_empty() || !cams.is_empty() || shared;
        p.set_visible(on);
        if !on {
            pp.close();
        }
        let shown = format!("{mics:?} {muted} {cams:?} {stoppable:?} {shared} {shares:?}");
        if memo.changed("menu", shown) {
            cards(&body, &mics, muted, &cams, &stoppable, shared.then_some(shares.as_slice()));
        }
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
    // ostrov privacy: its menu opened or closed, as a click does: for a script, a test
    let pp = pop.clone();
    let command: super::Command = Box::new(move |_| {
        pp.toggle();
        Ok(String::new())
    });
    Block { popup: Some(pop), command: Some(command), forms: &[""], ..Block::new(&s) }
}
