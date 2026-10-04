use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};

use crate::cc::{Ctx, Face, Widget};
use crate::hub::{s, service};
use crate::style::{clear, label};
use crate::ui::{arrow, menu, row, Memo, Slider};

/// Brightness, the night light behind its arrow: the mode, the hours, the warmth, shown on the screen while
/// dragged.
pub fn brightness(c: &Ctx) -> Widget {
    let bri = Slider::new("display-brightness-symbolic", |v| service(&["brightness", &format!("{}", (v * 100.0).round() as i64)]));
    bri.root.append(&arrow(c.flip.clone()));
    let (card, items) = menu("night-light-symbolic", "Night Light");
    let modes = gtk4::Box::new(Orientation::Vertical, 0);
    items.append(&modes);
    let hours = gtk4::Box::new(Orientation::Horizontal, 8);
    hours.set_margin_start(36);
    let from = gtk4::Entry::new();
    let to = gtk4::Entry::new();
    for (name, e) in [("from", &from), ("to", &to)] {
        e.set_max_width_chars(5);
        e.set_width_chars(5);
        hours.append(&label(name, "dim"));
        hours.append(e);
    }
    let (f2, t2) = (from.clone(), to.clone());
    let set_hours = Rc::new(move || service(&["night", "time", &f2.text(), &t2.text()]));
    let s1 = set_hours.clone();
    from.connect_activate(move |_| s1());
    to.connect_activate(move |_| set_hours());
    items.append(&hours);
    let warm_head = gtk4::Box::new(Orientation::Horizontal, 0);
    warm_head.set_margin_top(6);
    let wl = label("Warmth", "dim");
    wl.set_hexpand(true);
    let kelvin = label("", "dim");
    warm_head.append(&wl);
    warm_head.append(&kelvin);
    items.append(&warm_head);
    let save: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let k2 = kelvin.clone();
    let warm = Slider::new("night-light-symbolic", move |v| {
        let k = (6500.0 - v * 4000.0).round() as i64;
        k2.set_text(&format!("{k} K"));
        service(&["night", "preview", &k.to_string()]);
        // kept once the slider rests
        if let Some(id) = save.borrow_mut().take() {
            id.remove();
        }
        let s2 = save.clone();
        *save.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_millis(500), move || {
            s2.borrow_mut().take();
            service(&["night", "temp", &k.to_string()]);
        }));
    });
    items.append(&warm.root);
    let memo = Memo::default();
    let root = bri.root.clone();
    let face = Face::new(&bri.face);
    let w = Widget::new(&root, Some(&card), move |st| {
        bri.set(st["brightness"].as_f64().unwrap_or(0.0) / 100.0, "display-brightness-symbolic");
        let n = &st["night"];
        if !memo.changed("night", n.to_string()) {
            return;
        }
        clear(&modes);
        let mode = s(n, &["mode"]);
        let sun = if n["located"].as_bool().unwrap_or(false) {
            format!("{} – {}", s(n, &["sunset"]), s(n, &["sunrise"]))
        } else {
            "no location".into()
        };
        for (m, text, note) in [
            ("off", "Off", String::new()),
            ("on", "Always on", String::new()),
            ("time", "Scheduled", format!("{} – {}", s(n, &["from"]), s(n, &["to"]))),
            ("sun", "Sunset to sunrise", sun),
        ] {
            modes.append(&row("", text, &note, mode == m, move || service(&["night", "mode", m])));
        }
        hours.set_visible(mode == "time");
        from.set_text(s(n, &["from"]));
        to.set_text(s(n, &["to"]));
        let k = n["temp"].as_f64().unwrap_or(4000.0);
        warm.set((6500.0 - k) / 4000.0, "night-light-symbolic");
        kelvin.set_text(&format!("{k} K"));
    });
    Widget { face: Some(face), ..w }
}
