//! The control centre's Appearance page: the themes as cards in their own colours, the accent as swatches (or
//! any colour through GTK's colour dialog), then the rest of [appearance] as its form (settings/). Each pick is
//! written to config.toml at once and taken live: style.rs follows the file.


use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use serde_json::Value;

use crate::look::{ACCENTS, THEMES};
use crate::style::{clear, label};
use crate::ui::{header, setting};

/// The page; back leaves it.
pub fn page(back: impl Fn() + 'static) -> gtk4::Box {
    let root = gtk4::Box::new(Orientation::Vertical, 6);
    root.append(&header("Appearance", back).0);
    let body = gtk4::Box::new(Orientation::Vertical, 0);
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk4::PolicyType::Never);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(560);
    body.add_css_class("page-body");
    scroll.set_child(Some(&body));
    root.append(&scroll);
    fill(&body);
    let b = body.clone();
    crate::settings::on_outside(move || fill(&b));
    root
}

/// key of [appearance] set (None: the default again), the page drawn anew once the click that set it is over.
fn set(body: &gtk4::Box, key: &'static str, v: Option<Value>) {
    if let Err(e) = crate::settings::write("appearance", key, v.as_ref(), false) {
        eprintln!("ostrov: appearance: {e}");
    }
    let body = body.clone();
    glib::idle_add_local_once(move || fill(&body));
}

fn fill(body: &gtk4::Box) {
    clear(body);
    let a = crate::config::load().appearance;

    let themes = gtk4::Box::new(Orientation::Horizontal, 6);
    themes.set_homogeneous(true);
    for t in THEMES {
        let b = gtk4::Button::new();
        b.add_css_class("theme");
        b.add_css_class(&format!("theme-{t}"));
        if a.theme == *t {
            b.add_css_class("picked");
        }
        let card = gtk4::Box::new(Orientation::Vertical, 4);
        let dot = gtk4::Box::new(Orientation::Horizontal, 0);
        dot.add_css_class("theme-dot");
        dot.set_halign(gtk4::Align::Start);
        card.append(&dot);
        let mut name = t.to_string();
        name[..1].make_ascii_uppercase();
        let l = label(&name, "");
        l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        card.append(&l);
        b.set_child(Some(&card));
        let body = body.clone();
        b.connect_clicked(move |_| set(&body, "theme", Some((*t).into())));
        themes.append(&b);
    }
    body.append(&setting("Theme", "", &themes, true));

    let accents = gtk4::FlowBox::new();
    accents.set_selection_mode(gtk4::SelectionMode::None);
    accents.set_column_spacing(4);
    accents.set_row_spacing(6);
    accents.set_max_children_per_line(12);
    let own = gtk4::Button::with_label("Theme's");
    own.add_css_class("chip");
    own.set_tooltip_text(Some("The theme's own accent"));
    if a.accent.is_empty() {
        own.add_css_class("picked");
    }
    let b2 = body.clone();
    own.connect_clicked(move |_| set(&b2, "accent", None));
    accents.insert(&own, -1);
    for (i, c) in ACCENTS.iter().enumerate() {
        let b = gtk4::Button::new();
        b.add_css_class("swatch");
        b.add_css_class(&format!("swatch-{i}"));
        b.set_tooltip_text(Some(c));
        if a.accent.eq_ignore_ascii_case(c) {
            b.add_css_class("picked");
        }
        let body = body.clone();
        b.connect_clicked(move |_| set(&body, "accent", Some((*c).into())));
        accents.insert(&b, -1);
    }
    // any other colour, through GTK's dialog
    let custom = gtk4::ColorDialogButton::new(Some(gtk4::ColorDialog::new()));
    custom.add_css_class("swatch-custom");
    custom.set_halign(gtk4::Align::Start);
    custom.set_tooltip_text(Some("Another colour"));
    // the accent shown here, picked, when no swatch is it; clear otherwise
    let other = !a.accent.is_empty() && !ACCENTS.iter().any(|c| a.accent.eq_ignore_ascii_case(c));
    let rgba = gtk4::gdk::RGBA::parse(a.accent.as_str()).ok().filter(|_| other);
    custom.set_rgba(&rgba.unwrap_or(gtk4::gdk::RGBA::TRANSPARENT));
    if other {
        custom.add_css_class("picked");
    }
    let body2 = body.clone();
    custom.connect_rgba_notify(move |b| set(&body2, "accent", Some(crate::settings::form::hex(&b.rgba()).into())));
    accents.insert(&custom, -1);
    body.append(&setting("Accent", "", &accents, true));

    let mut rest = crate::settings::appearance().remove(0);
    rest.fields.retain(|f| f.key != "theme" && f.key != "accent");
    body.append(&crate::settings::form::section(&rest));
}
