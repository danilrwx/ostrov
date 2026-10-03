//! The clock in the middle, GNOME's "Sat Oct 3  13:31", the weather now before it; a click unrolls the calendar
//! out of it (calendar.rs).

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::glib;

use super::{pill, slot, Block, Ctx};

pub fn build(cx: &Rc<Ctx>) -> Block {
    let mid = pill();
    let weather_icon = gtk4::Image::new();
    let weather = gtk4::Label::new(None);
    let clock = gtk4::Label::new(None);
    mid.append(&weather_icon);
    mid.append(&weather);
    mid.append(&clock);
    let s = slot(&mid);

    let tick = move || {
        if let Ok(now) = glib::DateTime::now_local() {
            clock.set_text(&now.format("%a %b %-d  %H:%M").unwrap_or_default());
        }
        glib::ControlFlow::Continue
    };
    let _ = tick.clone()();
    glib::timeout_add_seconds_local(1, tick);

    cx.hub.on(move |st| {
        let w = st["weather"].as_object();
        weather_icon.set_visible(w.is_some());
        weather.set_visible(w.is_some());
        if let Some(w) = w {
            weather_icon.set_icon_name(w["icon"].as_str());
            weather.set_text(&format!("{}°", w["temp"]));
        }
    });

    let cal = crate::calendar::build(&cx.host, &cx.hub, &s, &cx.notes);
    let click = gtk4::GestureClick::new();
    let c = cal.clone();
    click.connect_released(move |_, _, _, _| c.toggle());
    mid.add_controller(click);
    mid.set_cursor_from_name(Some("pointer"));
    Block { popup: Some(cal), ..Block::new(&s) }
}
