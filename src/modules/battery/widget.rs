use std::rc::Rc;

use gtk4::prelude::*;
use serde_json::Value;

use crate::cc::{Ctx, Face, Widget};
use crate::hub::s;
use crate::style::label;
use crate::ui::battery_time;

/// The battery: its icon and percent, from three cells what is left of it too; its badge its icon, the percent
/// and what is left in its tooltip, there while a battery is.
pub fn battery(_: &Ctx) -> Widget {
    let bx = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    bx.add_css_class("battery");
    let icon = gtk4::Image::from_icon_name("battery-missing-symbolic");
    let pct = label("", "bold");
    let time = label("", "dim");
    time.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    bx.append(&icon);
    bx.append(&pct);
    bx.append(&time);
    let badge = gtk4::Image::from_icon_name("battery-missing-symbolic");
    let face = Face::new(&badge);
    let present = face.active.clone();
    let wide = Rc::new(std::cell::Cell::new(true));
    let (w2, t2, root) = (wide.clone(), time.clone(), bx.clone());
    let draw = move |st: &Value| {
        let b = &st["battery"];
        icon.set_icon_name(Some(s(b, &["icon"])));
        pct.set_text(&format!("{}%", b["percent"].as_f64().unwrap_or(0.0).round()));
        let t = battery_time(b);
        time.set_text(&t);
        time.set_visible(wide.get() && !t.is_empty());
        let has = b["present"].as_bool().unwrap_or(false);
        bx.set_visible(has);
        present.set(has);
        badge.set_icon_name(Some(s(b, &["icon"])));
        let t = if t.is_empty() { t } else { format!(", {t}") };
        badge.set_tooltip_text(Some(&format!("{}%{t}", b["percent"].as_f64().unwrap_or(0.0).round())));
    };
    Widget {
        face: Some(face),
        size: Box::new(move |w, _| {
            w2.set(w >= 3);
            t2.set_visible(w >= 3 && !t2.text().is_empty());
        }),
        ..Widget::new(&root, None, draw)
    }
}
