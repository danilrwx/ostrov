//! The control centre's Appearance page: the themes, built in and installed (theme.rs), as cards in their own
//! colours, the accent as swatches (or any colour through GTK's colour dialog), then the rest of [appearance] as its
//! form (settings/). Each pick is written to config.toml at once and taken live: style.rs follows the file.


use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use serde_json::Value;

use crate::i18n::t;
use crate::look::ACCENTS;
use crate::style::{clear, label};
use crate::ui::{header, setting};

/// The page; back leaves it.
pub fn page(back: impl Fn() + 'static) -> gtk4::Box {
    let root = gtk4::Box::new(Orientation::Vertical, 6);
    root.append(&header(t("Appearance"), back).0);
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk4::PolicyType::Never);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(560);
    scroll.set_child(Some(&body(&[])));
    root.append(&scroll);
    root
}

/// The page's body without its header: the themes, the accent, then of the rest of [appearance] the fields keep
/// names (none: all of them), drawn anew as the config changes. The first run's welcome has it too.
pub fn body(keep: &'static [&'static str]) -> gtk4::Box {
    let body = gtk4::Box::new(Orientation::Vertical, 0);
    body.add_css_class("page-body");
    fill(&body, keep);
    let b = body.clone();
    crate::settings::on_outside(move || fill(&b, keep));
    // a theme installed or removed (its card in or out), or picked by `ostrov theme set`
    let seen = std::cell::RefCell::new(themes());
    let b = body.clone();
    crate::style::on_config(move || {
        if seen.replace(themes()) != *seen.borrow() {
            fill(&b, keep);
        }
    });
    body
}

/// key of [appearance] set (None: the default again), the page drawn anew once the click that set it is over.
fn set(body: &gtk4::Box, keep: &'static [&'static str], key: &'static str, v: Option<Value>) {
    if let Err(e) = crate::settings::write("appearance", key, v.as_ref(), false) {
        eprintln!("ostrov: appearance: {e}");
    }
    let body = body.clone();
    glib::idle_add_local_once(move || fill(&body, keep));
}

/// The themes' ids in the page's order, and the one picked.
fn themes() -> (Vec<String>, String) {
    let a = crate::config::load().appearance;
    (crate::theme::all().into_iter().map(|t| t.id).collect(), format!("{} {}", crate::theme::canonical(&a.theme), crate::theme::light_mode()))
}

fn fill(body: &gtk4::Box, keep: &'static [&'static str]) {
    clear(body);
    let a = crate::config::load().appearance;

    // five to a row, as many rows as there are themes
    let themes = gtk4::FlowBox::new();
    themes.set_selection_mode(gtk4::SelectionMode::None);
    themes.set_homogeneous(true);
    themes.set_column_spacing(6);
    themes.set_row_spacing(6);
    themes.set_min_children_per_line(5);
    themes.set_max_children_per_line(5);
    for t in crate::theme::all() {
        let b = gtk4::Button::new();
        b.add_css_class("theme");
        b.add_css_class(&format!("theme-{}", t.id));
        b.set_tooltip_text(Some(format!("{}  {} {}", t.name, t.author, t.version).trim()));
        if crate::theme::canonical(&a.theme) == t.id {
            b.add_css_class("picked");
        }
        let card = gtk4::Box::new(Orientation::Vertical, 4);
        let dot = gtk4::Box::new(Orientation::Horizontal, 0);
        dot.add_css_class("theme-dot");
        dot.set_halign(gtk4::Align::Start);
        card.append(&dot);
        let l = label(&t.name, "");
        l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        card.append(&l);
        b.set_child(Some(&card));
        let body = body.clone();
        b.connect_clicked(move |_| {
            // the side an old id (light) gave kept, now that the id says no side
            if crate::config::load().appearance.mode.is_empty() {
                let mode = if crate::theme::light_mode() { "light" } else { "dark" };
                let _ = crate::settings::write("appearance", "mode", Some(&mode.into()), false);
            }
            set(&body, keep, "theme", Some(t.id.clone().into()))
        });
        themes.insert(&b, -1);
    }
    body.append(&setting(t("Theme"), "", &themes, true));

    // the theme's own and any other colour over the swatches, all one size: GTK's flow lines its children up in
    // columns
    let accents = crate::ui::chip_flow();
    accents.set_homogeneous(true);
    let accent = gtk4::Box::new(Orientation::Vertical, 6);
    let own = crate::ui::chip(t("Theme's"));
    own.set_tooltip_text(Some(t("The theme's own accent")));
    if a.accent.is_empty() {
        own.add_css_class("picked");
    }
    let b2 = body.clone();
    own.connect_clicked(move |_| set(&b2, keep, "accent", None));
    let line = gtk4::Box::new(Orientation::Horizontal, 6);
    line.append(&own);
    accent.append(&line);
    for (i, c) in ACCENTS.iter().enumerate() {
        let b = gtk4::Button::new();
        b.add_css_class("swatch");
        b.set_valign(gtk4::Align::Center);
        b.add_css_class(&format!("swatch-{i}"));
        b.set_tooltip_text(Some(c));
        if a.accent.eq_ignore_ascii_case(c) {
            b.add_css_class("picked");
        }
        let body = body.clone();
        b.connect_clicked(move |_| set(&body, keep, "accent", Some((*c).into())));
        crate::ui::flow_in(&accents, &b);
    }
    // any other colour, through GTK's dialog
    let custom = gtk4::ColorDialogButton::new(Some(gtk4::ColorDialog::new()));
    custom.add_css_class("swatch-custom");
    custom.set_halign(gtk4::Align::Start);
    custom.set_tooltip_text(Some(t("Another colour")));
    // the accent shown here, picked, when no swatch is it; clear otherwise
    let other = !a.accent.is_empty() && !ACCENTS.iter().any(|c| a.accent.eq_ignore_ascii_case(c));
    let rgba = gtk4::gdk::RGBA::parse(a.accent.as_str()).ok().filter(|_| other);
    custom.set_rgba(&rgba.unwrap_or(gtk4::gdk::RGBA::TRANSPARENT));
    if other {
        custom.add_css_class("picked");
    }
    let body2 = body.clone();
    custom.connect_rgba_notify(move |b| {
        set(&body2, keep, "accent", Some(crate::settings::form::hex(&b.rgba()).into()))
    });
    custom.set_valign(gtk4::Align::Center);
    line.append(&custom);
    accent.append(&accents);
    body.append(&setting(t("Accent"), "", &accent, true));

    let mut rest = crate::settings::appearance().remove(0);
    let kept = |k: &str| k != "theme" && k != "accent" && (keep.is_empty() || keep.contains(&k));
    rest.fields.retain(|f| kept(&f.key));
    body.append(&crate::settings::form::section(&rest));
    // the whole page (not the welcome's few): the apps ostrov colours by a file
    if keep.is_empty() {
        body.append(&apps());
    }
}

/// The apps on this machine ostrov colours by a file (integrations.rs), each a switch: its config made to read
/// ostrov's colours or not; one without a config to connect (Telegram's palette, to open once) says where it is.
fn apps() -> gtk4::Box {
    let col = gtk4::Box::new(Orientation::Vertical, 0);
    col.add_css_class("form-section");
    col.append(&label(t("Apps"), "title"));
    let help = label(t("Other apps in these colours, as they change: a switch makes the app's config read ostrov's file (~/.local/state/ostrov/colors)."), "field-help");
    help.set_wrap(true);
    col.append(&help);
    for a in crate::integrations::all().into_iter().filter(crate::integrations::App::here) {
        if a.include.is_none() {
            let note = crate::i18n::fill(t("Its file: {}"), &[&a.file().display()]);
            col.append(&setting(&a.name, &note, &gtk4::Box::new(Orientation::Horizontal, 0), false));
            continue;
        }
        let sw = gtk4::Switch::new();
        sw.set_active(a.connected());
        let id = a.id.clone();
        sw.connect_state_set(move |sw, on| {
            if let Err(e) = crate::integrations::set(&id, on) {
                eprintln!("ostrov: apps: {e}");
                return glib::Propagation::Stop;
            }
            sw.set_state(on);
            glib::Propagation::Stop
        });
        let where_ = a.include.as_ref().map_or(String::new(), |i| if i.link.is_empty() { i.file.clone() } else { i.link.clone() });
        col.append(&setting(&a.name, &where_, &sw, false));
    }
    col
}
