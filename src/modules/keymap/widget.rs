use std::rc::Rc;

use gtk4::prelude::*;
use serde_json::Value;

use super::service::shown;
use crate::cc::{Ctx, Face, Widget};
use crate::hub::service;
use crate::i18n::t;
use crate::settings::{Field, Kind, Opt, Schema, Section};
use crate::ui::{menu, row, Memo, Toggle};

/// How the layout shows: a flag, a language's letters, Hyprland's name for it.
pub fn settings() -> Schema {
    let opt = |v: &str, l: &str| Opt::Labeled { value: v.into(), label: l.into() };
    let show = Field::new("show", "Show", Kind::Choice {
        options: vec![opt("language", "Language: EN, RU"), opt("flag", "Flag: 🇺🇸 🇷🇺"), opt("code", "Code: us, ru")],
    })
    .default("language");
    Schema { sections: vec![Section::new("", "Keyboard Layout", vec![show])] }
}

/// [widget.keymap]'s show, read anew at every draw.
fn how() -> String {
    let cfg = crate::config::load();
    cfg.widget.get("keymap").and_then(|t| t.get("show")?.as_str().map(String::from)).unwrap_or_else(|| "language".into())
}

/// The layout: its flag or letters in the icon's place, its name under the title; a click the next layout, its menu
/// every one; its badge the same letters or flag, a click on it in the bar the next layout too.
pub fn keymap(c: &Ctx) -> Widget {
    let tg = Toggle::new("input-keyboard-symbolic", t("Keyboard Layout"), || service(&["keymap", "next"]), Some(c.flip.clone()));
    let (card, items) = menu("input-keyboard-symbolic", t("Keyboard Layout"));
    let badge = gtk4::Label::new(None);
    badge.add_css_class("keymap-badge");
    let face = Face::new(&badge);
    *face.click.borrow_mut() = Some(Rc::new(|| service(&["keymap", "next"])) as Rc<dyn Fn()>);
    let memo = Memo::default();
    let t2 = tg.clone();
    let w = Widget::toggle(&tg, Some(&card), move |st: &Value| {
        let k = &st["keymap"];
        let layouts: Vec<String> = k["layouts"].as_array().into_iter().flatten().filter_map(|l| l.as_str().map(String::from)).collect();
        let at = k["index"].as_u64().unwrap_or(0) as usize;
        let how = how();
        let now = layouts.get(at).map(|l| shown(l, &how)).unwrap_or_default();
        t2.glyph(&now);
        t2.set(false, "", k["name"].as_str().unwrap_or(""));
        badge.set_text(&now);
        badge.set_tooltip_text(k["name"].as_str());
        if !memo.changed("layouts", format!("{layouts:?}{at}{how}")) {
            return;
        }
        crate::style::clear(&items);
        for (i, l) in layouts.iter().enumerate() {
            let n = i.to_string();
            items.append(&row("", &shown(l, &how), l, i == at, move || service(&["keymap", "set", &n])));
        }
    });
    Widget { face: Some(face), ..w }
}
