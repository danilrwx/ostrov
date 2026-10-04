//! The time: a widget of the time large over the date, its badge the time as [widget.clock]'s format says
//! (GNOME's "Sat Oct 3  13:31" unless it says otherwise), ticking every second.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};

use super::{widget, Module};
use crate::cc::{Ctx, Face, Show, Widget};
use crate::i18n::t;
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
    let own = cfg.widget.get("clock").and_then(|t| t.get("format")?.as_str().map(String::from));
    own.unwrap_or_else(|| t(FORMAT).into())
}

const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

/// The time as the format says, its day and month names (%A %a %B %b) in ostrov's language: glib's would be
/// LC_TIME's, English under a locale apart from the language set. A short name is the full one's first three
/// letters in English; %B a month's as a date says it (its genitive in Russian, "3 октября").
pub fn format_time(now: &glib::DateTime, fmt: &str) -> String {
    let day = DAYS[(now.day_of_week() as usize).clamp(1, 7) - 1];
    let month = MONTHS[(now.month() as usize).clamp(1, 12) - 1];
    let mut out = String::with_capacity(fmt.len() + 16);
    let mut chars = fmt.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('A') => out.push_str(t(day)),
            Some('a') => out.push_str(t(&day[..3])),
            Some('B') => out.push_str(t(month)),
            Some('b') => out.push_str(t(&month[..3])),
            Some(o) => out.extend(['%', o]),
            None => out.push('%'),
        }
    }
    now.format(&out).map(|s| s.to_string()).unwrap_or_default()
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
            d.set_text(&format_time(&now, crate::i18n::t("%A, %B %-d")));
            b.set_text(&format_time(&now, &fmt.borrow()));
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

#[cfg(test)]
mod tests {
    use crate::i18n::t;

    #[test]
    fn names_put_in_and_the_rest_left_to_glib() {
        let at = gtk4::glib::DateTime::from_utc(2026, 10, 3, 13, 31, 0.0).unwrap();
        let want = format!("{} {} 3  13:31 %a", t("Sat"), t("Oct"));
        assert_eq!(super::format_time(&at, "%a %b %-d  %H:%M %%a"), want);
        assert_eq!(super::format_time(&at, "%A %B"), format!("{} {}", t("Saturday"), t("October")));
    }
}
