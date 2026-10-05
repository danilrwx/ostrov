//! The player now: its art, track, artist, previous/play/next, how far in (running on between its reports while
//! it plays); two cells wide its art alone, play/pause over it, the track in its hover; its badge, while it plays,
//! a note and the track.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};

use crate::cc::{Ctx, Face, Widget};
use crate::hub::s;
use crate::i18n::t;
use crate::style::label;

fn media(cmd: &str) {
    crate::hub::service(&["media", cmd]);
}

pub fn player(_: &Ctx) -> Widget {
    let player = gtk4::Box::new(Orientation::Horizontal, 10);
    player.add_css_class("card");
    let art = gtk4::Picture::new();
    art.set_size_request(56, 56);
    art.set_content_fit(gtk4::ContentFit::Cover);
    art.add_css_class("art");
    let pcol = gtk4::Box::new(Orientation::Vertical, 2);
    pcol.set_hexpand(true);
    let title = label(t("Nothing playing"), "bold");
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
    // the art, play/pause over it while the widget is two cells wide
    let cover = gtk4::Overlay::new();
    cover.set_child(Some(&art));
    let play2 = gtk4::Button::from_icon_name("media-playback-start-symbolic");
    play2.add_css_class("round");
    play2.add_css_class("art-play");
    play2.set_halign(gtk4::Align::Center);
    play2.set_valign(gtk4::Align::Center);
    play2.set_visible(false);
    play2.connect_clicked(|_| media("play-pause"));
    cover.add_overlay(&play2);
    player.append(&cover);
    player.append(&pcol);

    let badge = gtk4::Box::new(Orientation::Horizontal, 6);
    badge.append(&gtk4::Image::from_icon_name("audio-x-generic-symbolic"));
    let btitle = gtk4::Label::new(None);
    btitle.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    btitle.set_max_width_chars(24);
    badge.append(&btitle);
    let face = Face::new(&badge);
    let active = face.active.clone();

    // the position, run on between the player's reports
    let since: Rc<RefCell<(Instant, i64, i64, bool)>> = Rc::new(RefCell::new((Instant::now(), 0, 0, false)));
    let (s2, p2) = (since.clone(), progress.downgrade());
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        let Some(p) = p2.upgrade() else { return glib::ControlFlow::Break };
        let (at, pos, len, playing) = *s2.borrow();
        if len > 0 {
            let now = pos + if playing { at.elapsed().as_micros() as i64 } else { 0 };
            p.set_fraction((now as f64 / len as f64).clamp(0.0, 1.0));
        }
        glib::ControlFlow::Continue
    });
    let last_art = RefCell::new(String::new());
    let (pc, a2, p3, cv) = (pcol.clone(), art.clone(), play2.clone(), cover.clone());
    // under three cells its art alone, play over it; three and four its words and buttons, no art beside them
    // (they would be cut); five and more both
    let size = move |w: u8, _h: u8| {
        let small = w < 3;
        cv.set_visible(small || w >= 5);
        pc.set_visible(!small);
        p3.set_visible(small);
        a2.set_hexpand(small);
        a2.set_vexpand(small);
    };
    Widget {
        face: Some(face),
        size: Box::new(size),
        ..Widget::new(&player.clone(), None, move |st| {
            let m = &st["media"];
            let has = !s(m, &["name"]).is_empty();
            ctl.set_sensitive(has);
            let playing = has && m["playing"].as_bool().unwrap_or(false);
            active.set(playing);
            if !has {
                title.set_text(t("Nothing playing"));
                artist.set_visible(false);
                progress.set_visible(false);
                art.set_paintable(None::<&gtk4::gdk::Paintable>);
                last_art.borrow_mut().clear();
                return;
            }
            let t = s(m, &["title"]);
            let t = if t.is_empty() { s(m, &["identity"]) } else { t };
            title.set_text(t);
            btitle.set_text(t);
            artist.set_text(s(m, &["artist"]));
            artist.set_visible(!s(m, &["artist"]).is_empty());
            let icon = if playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" };
            play.set_icon_name(icon);
            play2.set_icon_name(icon);
            let who = s(m, &["artist"]);
            player.set_tooltip_text(Some(&if who.is_empty() { t.to_string() } else { format!("{t} — {who}") }));
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
        })
    }
}
