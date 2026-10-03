//! The calendar grown out of the bar's clock, as the Quickshell bar's date menu: at the left the player (MPRIS:
//! art, track, artist, previous/play/next, how far in) over the notifications' history with Do Not Disturb and
//! Clear; at the right the month (GTK's calendar, today inverted, the days with events marked; its top level with
//! the player's), the picked day's events (services/calendar.rs: a CalDAV account's, .ics links'), the weather
//! where wmd location put the machine. Everything from wmd's state but the history, ostrov's own notifications.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};

use crate::style::label;
use crate::hub::{s, Hub};
use crate::notes::Notes;
use crate::popup::{Popup, Side};


fn media(cmd: &str) {
    crate::hub::service(&["media", cmd]);
}

pub fn build(host: &Rc<crate::popup::Host>, hub: &Rc<Hub>, tab: &impl IsA<gtk4::Widget>, notes: &Rc<Notes>) -> Rc<Popup> {
    let body = gtk4::Box::new(Orientation::Horizontal, 14);
    body.add_css_class("surface");

    // the left: the player, the notifications
    let left = gtk4::Box::new(Orientation::Vertical, 8);
    left.set_hexpand(true);
    left.set_size_request(300, -1);

    let player = gtk4::Box::new(Orientation::Horizontal, 10);
    player.add_css_class("card");
    let art = gtk4::Picture::new();
    art.set_size_request(56, 56);
    art.set_content_fit(gtk4::ContentFit::Cover);
    art.add_css_class("art");
    let pcol = gtk4::Box::new(Orientation::Vertical, 2);
    pcol.set_hexpand(true);
    let title = label("", "bold");
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let artist = label("", "dim");
    artist.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let ctl = gtk4::Box::new(Orientation::Horizontal, 4);
    let prev = gtk4::Button::from_icon_name("media-skip-backward-symbolic");
    let play = gtk4::Button::from_icon_name("media-playback-start-symbolic");
    let next = gtk4::Button::from_icon_name("media-skip-forward-symbolic");
    for (b, cmd) in [(&prev, "previous"), (&play, "play-pause"), (&next, "next")] {
        b.add_css_class("flat-round");
        b.connect_clicked(move |_| media(cmd));
        ctl.append(b);
    }
    let progress = gtk4::ProgressBar::new();
    progress.add_css_class("progress");
    pcol.append(&title);
    pcol.append(&artist);
    pcol.append(&ctl);
    pcol.append(&progress);
    player.append(&art);
    player.append(&pcol);
    left.append(&player);

    left.append(&label("Notifications", "title"));
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_min_content_height(200);
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    let list = gtk4::Box::new(Orientation::Vertical, 6);
    scroll.set_child(Some(&list));
    left.append(&scroll);
    let foot = gtk4::Box::new(Orientation::Horizontal, 8);
    foot.set_valign(Align::End);
    let dnd = gtk4::ToggleButton::with_label("Do Not Disturb");
    dnd.add_css_class("chip");
    let spacer = gtk4::Box::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    let clear = gtk4::Button::with_label("Clear");
    dnd.set_valign(Align::Center);
    clear.set_valign(Align::Center);
    clear.add_css_class("chip");
    foot.append(&dnd);
    foot.append(&spacer);
    foot.append(&clear);
    left.append(&foot);
    body.append(&left);

    body.append(&gtk4::Separator::new(Orientation::Vertical));

    // the right: the date, the month, the weather
    let right = gtk4::Box::new(Orientation::Vertical, 4);
    right.set_size_request(300, -1);
    let cal = gtk4::Calendar::new();
    right.append(&cal);
    let agenda = gtk4::Box::new(Orientation::Vertical, 6);
    agenda.add_css_class("card");
    agenda.set_margin_top(8);
    // shown only with a day's events in it (draw below)
    agenda.set_visible(false);
    right.append(&agenda);
    let weather = gtk4::Box::new(Orientation::Vertical, 8);
    weather.add_css_class("card");
    weather.set_margin_top(8);
    let wnow = gtk4::Box::new(Orientation::Horizontal, 10);
    let wicon = gtk4::Image::new();
    wicon.set_pixel_size(28);
    let wcol = gtk4::Box::new(Orientation::Vertical, 0);
    let wtemp = label("", "bold");
    let wplace = label("", "dim");
    wcol.append(&wtemp);
    wcol.append(&wplace);
    wnow.append(&wicon);
    wnow.append(&wcol);
    let hours = gtk4::Box::new(Orientation::Horizontal, 0);
    hours.set_homogeneous(true);
    weather.append(&wnow);
    weather.append(&hours);
    right.append(&weather);
    body.append(&right);

    let popup = Popup::new(host, tab, Side::Center, 680, &body);

    // on every opening: today, this month
    let c2 = cal.clone();
    let today = move || {
        if let Ok(now) = glib::DateTime::now_local() {
            c2.select_day(&now);
        }
    };
    today();
    popup.on_open(today);

    // the events: the month's days with any marked, the picked day's listed; nothing while no calendar is set or
    // the day has none
    {
        let events: Rc<RefCell<serde_json::Value>> = Rc::default();
        let (c2, a2, e2) = (cal.clone(), agenda.clone(), events.clone());
        let draw: Rc<dyn Fn()> = Rc::new(move || {
            let events = e2.borrow();
            c2.clear_marks();
            crate::style::clear(&a2);
            let Some(events) = events.as_array() else { return a2.set_visible(false) };
            let picked = c2.date();
            let Ok(first) = picked.add_days(1 - picked.day_of_month()) else { return };
            for d in (0..31).filter_map(|i| first.add_days(i).ok()).take_while(|d| d.month() == picked.month()) {
                if events.iter().any(|e| on(e, &d)) {
                    c2.mark_day(d.day_of_month() as u32);
                }
            }
            // no card at all for a day without events: nothing empty between the month and the weather
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
        hub.on(move |st| {
            if *events.borrow() != st["calendar"] {
                *events.borrow_mut() = st["calendar"].clone();
                draw();
            }
        });
    }

    // the notifications' history: newest first, a click dismisses one, its actions as buttons
    {
        let (list, n2) = (list.clone(), notes.clone());
        let draw = move || {
            crate::style::clear(&list);
            let hist = n2.history();
            if hist.is_empty() {
                let l = label("No notifications", "dim");
                l.set_xalign(0.5);
                l.set_margin_top(40);
                list.append(&l);
            }
            for n in hist.iter().rev() {
                list.append(&n2.card(n, true));
            }
        };
        let d: Rc<dyn Fn()> = Rc::new(draw);
        d();
        let d2 = d.clone();
        notes.on_change(move || d2());
        let n3 = notes.clone();
        dnd.connect_toggled(move |b| n3.set_dnd(b.is_active()));
        let n4 = notes.clone();
        clear.connect_clicked(move |_| n4.clear());
    }

    // the player and the weather from wmd; the position runs on between its reports while it plays
    let since: Rc<RefCell<(Instant, i64, i64, bool)>> = Rc::new(RefCell::new((Instant::now(), 0, 0, false)));
    {
        let since = since.clone();
        let p2 = progress.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let (at, pos, len, playing) = *since.borrow();
            if len > 0 {
                let now = pos + if playing { at.elapsed().as_micros() as i64 } else { 0 };
                p2.set_fraction((now as f64 / len as f64).clamp(0.0, 1.0));
            }
            glib::ControlFlow::Continue
        });
    }
    let last_art = RefCell::new(String::new());
    hub.on(move |st| {
        let m = &st["media"];
        let has = !s(m, &["name"]).is_empty();
        player.set_visible(has);
        if has {
            let t = s(m, &["title"]);
            title.set_text(if t.is_empty() { s(m, &["identity"]) } else { t });
            artist.set_text(s(m, &["artist"]));
            artist.set_visible(!s(m, &["artist"]).is_empty());
            let playing = m["playing"].as_bool().unwrap_or(false);
            play.set_icon_name(if playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
            prev.set_sensitive(m["canPrev"].as_bool().unwrap_or(false));
            next.set_sensitive(m["canNext"].as_bool().unwrap_or(false));
            let len = m["length"].as_i64().unwrap_or(0);
            progress.set_visible(len > 0);
            *since.borrow_mut() = (Instant::now(), m["position"].as_i64().unwrap_or(0), len, playing);
            let a = s(m, &["art"]).to_string();
            if *last_art.borrow() != a {
                *last_art.borrow_mut() = a.clone();
                if a.is_empty() {
                    art.set_paintable(None::<&gtk4::gdk::Paintable>);
                } else {
                    art.set_file(Some(&gtk4::gio::File::for_uri(&a)));
                }
            }
        }

        let w = &st["weather"];
        weather.set_visible(w.is_object());
        if w.is_object() {
            wicon.set_icon_name(w["icon"].as_str());
            wtemp.set_text(&format!("{}°  {}", w["temp"], s(w, &["text"])));
            wplace.set_text(&format!("{}   ↑{}° ↓{}°", s(w, &["place"]), w["high"], w["low"]));
            crate::style::clear(&hours);
            for h in w["hours"].as_array().into_iter().flatten() {
                let col = gtk4::Box::new(Orientation::Vertical, 2);
                let t = label(s(h, &["time"]), "dim");
                t.set_xalign(0.5);
                let i = gtk4::Image::from_icon_name(s(h, &["icon"]));
                let deg = label(&format!("{}°", h["temp"]), "");
                deg.set_xalign(0.5);
                col.append(&t);
                col.append(&i);
                col.append(&deg);
                col.set_halign(Align::Fill);
                hours.append(&col);
            }
        }
    });

    popup
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
