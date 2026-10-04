//! The calendars' widgets: the month (today inverted, the days with events marked, a day picked showing its
//! events under it) and the coming events, its badge the next one while it is near.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use serde_json::Value;

use crate::cc::{Ctx, Face, Widget};
use crate::hub::s;
use crate::style::{clear, label};

/// The month, the picked day's events under it (none, no card).
pub fn month(_: &Ctx) -> Widget {
    let col = gtk4::Box::new(Orientation::Vertical, 4);
    let cal = gtk4::Calendar::new();
    col.append(&cal);
    let agenda = gtk4::Box::new(Orientation::Vertical, 6);
    agenda.add_css_class("card");
    agenda.set_visible(false);
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&agenda));
    col.append(&scroll);

    let events: Rc<RefCell<Value>> = Rc::default();
    let (c2, a2, e2) = (cal.clone(), agenda.clone(), events.clone());
    let draw: Rc<dyn Fn()> = Rc::new(move || {
        let events = e2.borrow();
        c2.clear_marks();
        clear(&a2);
        let Some(events) = events.as_array() else { return a2.set_visible(false) };
        let picked = c2.date();
        let Ok(first) = picked.add_days(1 - picked.day_of_month()) else { return };
        for d in (0..31).filter_map(|i| first.add_days(i).ok()).take_while(|d| d.month() == picked.month()) {
            if events.iter().any(|e| on(e, &d)) {
                c2.mark_day(d.day_of_month() as u32);
            }
        }
        let day: Vec<_> = events.iter().filter(|e| on(e, &picked)).collect();
        a2.set_visible(!day.is_empty());
        for e in day {
            a2.append(&event(e, &picked));
        }
    });
    for sig in ["day-selected", "next-month", "prev-month", "next-year", "prev-year"] {
        let d = draw.clone();
        cal.connect_local(sig, false, move |_| {
            d();
            None
        });
    }
    // today again whenever the panel opens: the widget mapped anew
    let c3 = cal.clone();
    col.connect_map(move |_| {
        if let Ok(now) = glib::DateTime::now_local() {
            c3.select_day(&now);
        }
    });
    Widget::new(&col, None, move |st| {
        if *events.borrow() != st["calendar"] {
            *events.borrow_mut() = st["calendar"].clone();
            draw();
        }
    })
}

/// The events coming, today's and on, a few; its badge the next while it is within half an hour or on. Drawn
/// again every half minute as well as on every state, time going by without the state changing.
pub fn agenda(c: &Ctx) -> Widget {
    let col = gtk4::Box::new(Orientation::Vertical, 6);
    col.add_css_class("card");
    let list = gtk4::Box::new(Orientation::Vertical, 6);
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    col.append(&label("Coming Up", "title"));
    col.append(&scroll);
    let badge = gtk4::Label::new(None);
    badge.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    badge.set_max_width_chars(28);
    let face = Face::new(&badge);
    let active = face.active.clone();
    let draw = Rc::new(move |st: &Value| {
            let Ok(now) = glib::DateTime::now_local() else { return };
            let at = now.format("%Y-%m-%dT%H:%M:%S").map(|s| s.to_string()).unwrap_or_default();
            let soon = now.add_minutes(30).and_then(|t| t.format("%Y-%m-%dT%H:%M:%S")).map(|s| s.to_string()).unwrap_or_default();
            clear(&list);
            let coming: Vec<&Value> = st["calendar"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|e| s(e, &["end"]) > at.as_str() && e["all_day"] != true)
                .take(6)
                .collect();
            if coming.is_empty() {
                list.append(&label("Nothing coming", "dim"));
            }
            for e in &coming {
                let day = glib::DateTime::from_iso8601(s(e, &["start"]), Some(&glib::TimeZone::local())).unwrap_or(now.clone());
                list.append(&event(e, &day));
            }
            // the next one, near or on now
            let next = coming.first().filter(|e| s(e, &["start"]) <= soon.as_str());
            active.set(next.is_some());
            if let Some(e) = next {
                badge.set_text(&format!("{}  {}", s(e, &["start"]).get(11..16).unwrap_or(""), s(e, &["title"])));
            }
    });
    let (d2, st, alive) = (draw.clone(), c.state.clone(), col.downgrade());
    glib::timeout_add_seconds_local(30, move || {
        if alive.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        d2(&st.borrow());
        glib::ControlFlow::Continue
    });
    Widget { face: Some(face), ..Widget::new(&col, None, move |st| draw(st)) }
}

/// A day's bounds as the events' ISO times: its midnight and the next one.
fn bounds(day: &glib::DateTime) -> (String, String) {
    let iso = |d: &glib::DateTime| d.format("%Y-%m-%dT00:00:00").map(|s| s.to_string()).unwrap_or_default();
    (iso(day), day.add_days(1).map(|d| iso(&d)).unwrap_or_default())
}

/// Whether an event of the calendar service's falls on a day: overlaps it, or, taking no time, starts in it.
fn on(e: &serde_json::Value, day: &glib::DateTime) -> bool {
    let (from, to) = bounds(day);
    let (start, end) = (s(e, &["start"]), s(e, &["end"]));
    start < to.as_str() && (end > from.as_str() || start >= from.as_str())
}

/// An event's row: its hours on the day (a part running over from another day or on to the next ending or
/// starting at the day's edge), a dot of its calendar's colour, its title over where it is.
fn event(e: &serde_json::Value, day: &glib::DateTime) -> gtk4::Box {
    let (from, to) = bounds(day);
    fn hour<'a>(t: &'a str, from: &str, to: &str) -> &'a str {
        if t < from || t >= to { "…" } else { t.get(11..16).unwrap_or("") }
    }
    let when = if e["all_day"].as_bool() == Some(true) {
        "all day".to_string()
    } else {
        format!("{}–{}", hour(s(e, &["start"]), &from, &to), hour(s(e, &["end"]), &from, &to))
    };
    let row = gtk4::Box::new(Orientation::Horizontal, 8);
    let time = label(&when, "dim");
    time.set_width_chars(11);
    time.set_valign(Align::Start);
    // the calendar's own colour, so drawn rather than styled; the text's where it says none
    let dot = gtk4::DrawingArea::new();
    dot.set_content_width(8);
    dot.set_content_height(8);
    dot.set_valign(Align::Start);
    dot.set_margin_top(5);
    let color = gtk4::gdk::RGBA::parse(s(e, &["color"])).ok();
    dot.set_draw_func(move |w, cr, wd, ht| {
        let c = color.unwrap_or_else(|| w.color());
        cr.set_source_rgba(c.red().into(), c.green().into(), c.blue().into(), c.alpha().into());
        cr.arc(f64::from(wd) / 2.0, f64::from(ht) / 2.0, 4.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    });
    let col = gtk4::Box::new(Orientation::Vertical, 0);
    col.set_hexpand(true);
    let title = label(s(e, &["title"]), "");
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    col.append(&title);
    if !s(e, &["location"]).is_empty() {
        let place = label(s(e, &["location"]), "dim");
        place.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        col.append(&place);
    }
    row.append(&time);
    row.append(&dot);
    row.append(&col);
    row
}
