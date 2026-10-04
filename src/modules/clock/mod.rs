//! The time: a widget of the time large over the date, its badge the time as [widget.clock]'s format says
//! (GNOME's "Sat Oct 3  13:31" unless it says otherwise), ticking every second.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};

use super::{widget, Module};
use crate::cc::{Ctx, Face, Show, Widget};
use crate::settings::{Field, Kind, Schema, Section};
use crate::style::label;

pub const MODULE: Module = Module {
    id: "clock",
    widgets: &[widget("clock", "Clock", "preferences-system-time-symbolic", &[(4, 1), (2, 1), (8, 1), (4, 2)], clock)
        .bar(Show::Always)
        .settings(settings)],
    ..Module::NONE
};

const FORMAT: &str = "%a %b %-d  %H:%M";

fn settings() -> Schema {
    let format = Field::new("format", "Format in the bar", Kind::String)
        .default(FORMAT)
        .help("strftime's: %H:%M the time alone, %a %-d %b the day, %I:%M %p twelve hours.");
    Schema { sections: vec![Section::new("", "Clock", vec![format])] }
}

/// The format [widget.clock] says, read anew as the config changes.
fn format() -> String {
    let cfg = crate::config::load();
    cfg.widget.get("clock").and_then(|t| t.get("format")?.as_str().map(String::from)).unwrap_or(FORMAT.into())
}

fn clock(_: &Ctx) -> Widget {
    let col = gtk4::Box::new(Orientation::Vertical, 0);
    col.add_css_class("clock");
    col.set_valign(gtk4::Align::Center);
    let time = label("", "clock-time");
    let date = label("", "dim");
    col.append(&time);
    col.append(&date);
    let badge = gtk4::Label::new(None);
    let fmt = Rc::new(RefCell::new(format()));
    let f2 = fmt.clone();
    crate::style::on_config(move || *f2.borrow_mut() = format());
    // ticking while the widget is: its labels let go of, the tick stops
    let (t, d, b) = (time.downgrade(), date.downgrade(), badge.downgrade());
    let tick = move || {
        let (Some(t), Some(d), Some(b)) = (t.upgrade(), d.upgrade(), b.upgrade()) else { return glib::ControlFlow::Break };
        if let Ok(now) = glib::DateTime::now_local() {
            t.set_text(&now.format("%H:%M").unwrap_or_default());
            d.set_text(&now.format("%A, %B %-d").unwrap_or_default());
            b.set_text(&now.format(&fmt.borrow()).unwrap_or_default());
        }
        glib::ControlFlow::Continue
    };
    tick();
    glib::timeout_add_seconds_local(1, tick);
    let (d2, c2) = (date.clone(), col.clone());
    Widget {
        face: Some(Face::new(&badge)),
        // a row high, the date beside the time (under it it would not fit a dense row); from four cells wide
        size: Box::new(move |w, h| {
            d2.set_visible(w >= 4);
            c2.set_orientation(if h == 1 { Orientation::Horizontal } else { Orientation::Vertical });
            c2.set_spacing(if h == 1 { 12 } else { 0 });
            d2.set_valign(if h == 1 { gtk4::Align::Center } else { gtk4::Align::Fill });
        }),
        ..Widget::new(&col, None, |_| ())
    }
}
