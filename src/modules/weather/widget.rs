//! The weather where the machine is: now, the day's high and low, the hours coming; its badge the sky's icon and
//! the temperature.

use gtk4::prelude::*;
use gtk4::{Align, Orientation};

use crate::cc::{Ctx, Face, Widget};
use crate::hub::s;
use crate::style::{clear, label};

pub fn weather(_: &Ctx) -> Widget {
    let card = gtk4::Box::new(Orientation::Vertical, 4);
    card.set_valign(Align::Center);
    card.add_css_class("card");
    let now = gtk4::Box::new(Orientation::Horizontal, 10);
    let icon = gtk4::Image::new();
    icon.set_pixel_size(22);
    let col = gtk4::Box::new(Orientation::Vertical, 0);
    let temp = label("", "bold");
    let place = label("", "dim");
    col.append(&temp);
    col.append(&place);
    now.append(&icon);
    now.append(&col);
    let hours = gtk4::Box::new(Orientation::Horizontal, 0);
    hours.set_homogeneous(true);
    card.append(&now);
    card.append(&hours);

    let badge = gtk4::Box::new(Orientation::Horizontal, 6);
    let bicon = gtk4::Image::new();
    let btemp = gtk4::Label::new(None);
    badge.append(&bicon);
    badge.append(&btemp);
    let face = Face::new(&badge);
    let active = face.active.clone();
    let h2 = hours.clone();
    Widget {
        face: Some(face),
        size: Box::new(move |_, h| h2.set_visible(h >= 2)),
        ..Widget::new(&card.clone(), None, move |st| {
            let w = &st["weather"];
            active.set(w.is_object());
            card.set_visible(w.is_object());
            if !w.is_object() {
                return;
            }
            bicon.set_icon_name(w["icon"].as_str());
            btemp.set_text(&format!("{}°", w["temp"]));
            icon.set_icon_name(w["icon"].as_str());
            temp.set_text(&format!("{}°  {}", w["temp"], s(w, &["text"])));
            place.set_text(&format!("{}   ↑{}° ↓{}°", s(w, &["place"]), w["high"], w["low"]));
            clear(&hours);
            // an hour in two lines: its time over its sky and temperature side by side
            for h in w["hours"].as_array().into_iter().flatten() {
                let col = gtk4::Box::new(Orientation::Vertical, 0);
                let t = label(s(h, &["time"]), "hour");
                t.set_xalign(0.5);
                let line = gtk4::Box::new(Orientation::Horizontal, 3);
                line.set_halign(Align::Center);
                line.append(&gtk4::Image::from_icon_name(s(h, &["icon"])));
                line.append(&label(&format!("{}°", h["temp"]), ""));
                col.append(&t);
                col.append(&line);
                hours.append(&col);
            }
        })
    }
}
