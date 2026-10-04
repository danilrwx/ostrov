use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::Orientation;
use serde_json::Value;

use crate::cc::{Ctx, Widget};
use crate::hub::{s, service, service_then};
use crate::style::{clear, label};
use crate::ui::{menu, on_right_click, row, Memo, Toggle};

pub fn bluetooth(c: &Ctx) -> Widget {
    let st = c.state.clone();
    let t = Toggle::new("bluetooth-active-symbolic", "Bluetooth", move || {
        let on = st.borrow()["bt"]["on"].as_bool().unwrap_or(false);
        service(&["bt", if on { "off" } else { "on" }]);
    }, Some(c.flip.clone()));
    let (card, items) = menu("bluetooth-active-symbolic", "Bluetooth");
    // the device being paired, what went wrong pairing, a scan running, kept across redraws
    let pairing: Rc<RefCell<String>> = Rc::default();
    let error: Rc<RefCell<String>> = Rc::default();
    let scanning = Rc::new(RefCell::new(false));
    let memo = Memo::default();
    let again = c.again.clone();
    let t2 = t.clone();
    Widget::toggle(&t, Some(&card), move |st| {
        let b = &st["bt"];
        let on = b["on"].as_bool().unwrap_or(false);
        t2.set(on, if on { "bluetooth-active-symbolic" } else { "bluetooth-disabled-symbolic" }, s(b, &["connected"]));
        let key = format!("{}{}{}{}{}", b["devices"], b["discovering"], pairing.borrow(), error.borrow(), scanning.borrow());
        if !memo.changed("bt", key) {
            return;
        }
        clear(&items);
        let icon = |d: &Value| format!("{}-symbolic", if s(d, &["icon"]).is_empty() { "bluetooth" } else { s(d, &["icon"]) });
        let devs: Vec<&Value> = b["devices"].as_array().into_iter().flatten().collect();
        for d in devs.iter().filter(|d| d["paired"].as_bool().unwrap_or(false)) {
            let (addr, on) = (s(d, &["address"]).to_string(), d["connected"].as_bool().unwrap_or(false));
            let a2 = addr.clone();
            let r = row(&icon(d), s(d, &["name"]), "", on, move || service(&["bt", if on { "disconnect" } else { "connect" }, &a2]));
            on_right_click(&r, move || service(&["bt", "forget", &addr]));
            items.append(&r);
        }
        items.append(&gtk4::Separator::new(Orientation::Horizontal));
        let busy = b["discovering"].as_bool().unwrap_or(false) || *scanning.borrow();
        let (sc, again2) = (scanning.clone(), again.clone());
        items.append(&row("system-search-symbolic", if busy { "Scanning…" } else { "Scan for Devices" }, "", busy, move || {
            if *sc.borrow() {
                return;
            }
            *sc.borrow_mut() = true;
            let (sc, again) = (sc.clone(), again2.clone());
            service_then(vec!["bt".into(), "scan".into()], None, move |_| {
                *sc.borrow_mut() = false;
                again();
            });
            again2();
        }));
        for d in devs.iter().filter(|d| !d["paired"].as_bool().unwrap_or(false)) {
            let addr = s(d, &["address"]).to_string();
            let note = if *pairing.borrow() == addr { "pairing…" } else { "pair" };
            let (pa, err, again2) = (pairing.clone(), error.clone(), again.clone());
            items.append(&row(&icon(d), s(d, &["name"]), note, false, move || {
                if !pa.borrow().is_empty() {
                    return;
                }
                *pa.borrow_mut() = addr.clone();
                err.borrow_mut().clear();
                let (pa, err, again) = (pa.clone(), err.clone(), again2.clone());
                service_then(vec!["bt".into(), "pair".into(), addr.clone()], None, move |r| {
                    pa.borrow_mut().clear();
                    if let Err(e) = r {
                        *err.borrow_mut() = e;
                    }
                    again();
                });
                again2();
            }));
        }
        if !error.borrow().is_empty() {
            items.append(&label(&error.borrow(), "error"));
        }
    })
}

/// The devices connected that tell their charge: each its icon, name and percent with a bar; its badge the first
/// one's icon and percent, while one is connected.
pub fn batteries(_: &Ctx) -> Widget {
    let col = gtk4::Box::new(Orientation::Vertical, 6);
    col.add_css_class("card");
    col.set_valign(gtk4::Align::Center);
    let badge = gtk4::Box::new(Orientation::Horizontal, 4);
    let bicon = gtk4::Image::from_icon_name("audio-headphones-symbolic");
    let bpct = gtk4::Label::new(None);
    badge.append(&bicon);
    badge.append(&bpct);
    let face = crate::cc::Face::new(&badge);
    let active = face.active.clone();
    let memo = Memo::default();
    Widget {
        face: Some(face),
        ..Widget::new(&col.clone(), None, move |st| {
            let devs: Vec<&Value> = st["bt"]["devices"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["connected"] == true && d["battery"].is_u64())
                .collect();
            active.set(!devs.is_empty());
            let icon = |d: &Value| {
                let i = s(d, &["icon"]);
                format!("{}-symbolic", if i.is_empty() { "bluetooth" } else { i })
            };
            if let Some(d) = devs.first() {
                bicon.set_icon_name(Some(&icon(d)));
                bpct.set_text(&format!("{}%", d["battery"]));
                badge.set_tooltip_text(Some(s(d, &["name"])));
            }
            let key: String = devs.iter().map(|d| format!("{}{}", d["address"], d["battery"])).collect();
            if !memo.changed("devs", key) {
                return;
            }
            clear(&col);
            if devs.is_empty() {
                col.append(&label("No device tells its charge", "dim"));
            }
            for d in devs {
                let line = gtk4::Box::new(Orientation::Horizontal, 8);
                line.append(&gtk4::Image::from_icon_name(&icon(d)));
                let name = label(s(d, &["name"]), "");
                name.set_hexpand(true);
                name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                line.append(&name);
                line.append(&label(&format!("{}%", d["battery"]), "dim"));
                col.append(&line);
                let bar = gtk4::ProgressBar::new();
                bar.add_css_class("progress");
                bar.set_fraction(d["battery"].as_f64().unwrap_or(0.0) / 100.0);
                col.append(&bar);
            }
        })
    }
}
