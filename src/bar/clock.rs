//! The clock in the middle, GNOME's "Sat Oct 3  13:31", the weather now before it, the next event in its tooltip;
//! a click unrolls the calendar out of it (calendar.rs).

use std::cell::RefCell;
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

    // the calendar service's events, sorted by their starts
    let events: Rc<RefCell<serde_json::Value>> = Rc::default();
    let (m2, e2) = (mid.clone(), events.clone());
    let tick = move || {
        if let Ok(now) = glib::DateTime::now_local() {
            clock.set_text(&now.format("%a %b %-d  %H:%M").unwrap_or_default());
            let tip = next(&e2.borrow(), &now);
            if m2.tooltip_text().as_deref() != tip.as_deref() {
                m2.set_tooltip_text(tip.as_deref());
            }
        }
        glib::ControlFlow::Continue
    };
    let _ = tick.clone()();
    glib::timeout_add_seconds_local(1, tick);

    cx.hub.on(move |st| {
        if *events.borrow() != st["calendar"] {
            *events.borrow_mut() = st["calendar"].clone();
        }
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

/// The next event that is not a day's, as "14:00 Title", its day said too when not today's.
fn next(events: &serde_json::Value, now: &glib::DateTime) -> Option<String> {
    let at = now.format("%Y-%m-%dT%H:%M:%S").ok()?;
    let e = events.as_array()?.iter().find(|e| e["all_day"] != true && e["start"].as_str() > Some(at.as_str()))?;
    let start = glib::DateTime::from_iso8601(e["start"].as_str()?, Some(&glib::TimeZone::local())).ok()?;
    let fmt = if start.ymd() == now.ymd() { "%H:%M" } else { "%a %b %-d  %H:%M" };
    Some(format!("{}  {}", start.format(fmt).ok()?, e["title"].as_str().unwrap_or_default()))
}
