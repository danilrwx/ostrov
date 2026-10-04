//! The control centre's Displays: the toggle (on while more than one screen is, its note the profile of the
//! monitors connected) and its menu, from the displays service's state. Each monitor a row that turns it on or
//! off; under one that is on its modes in a dropdown, its scales, and, for any but the main one, where it goes
//! beside the main one or which it mirrors. Then the profiles saved, a click loading one, the bin deleting it, and
//! an entry to save the layout now under a name.

use gtk4::prelude::*;
use gtk4::Orientation;
use serde_json::Value;

use crate::cc::{Ctx, Widget};
use crate::ui::{chips, menu, row, Memo, Toggle};
use crate::hub::{s, service, service_then};
use crate::style::{clear, label};

const SCALES: [f64; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];

fn draw(d: &Value, toggle: &Toggle, items: &gtk4::Box, changed: bool) {
    let mons: Vec<Value> = d["monitors"].as_array().cloned().unwrap_or_default();
    toggle.root.set_visible(!mons.is_empty());
    let on = mons.iter().filter(|m| m["enabled"] == true).count();
    let profile = s(d, &["profile"]);
    let sub = if profile.is_empty() { format!("{on} on") } else { profile.to_string() };
    toggle.set(on > 1, "video-display-symbolic", &sub);
    if !changed {
        return;
    }
    clear(items);
    let main = mons.iter().find(|m| m["main"] == true).map(|m| s(m, &["name"]).to_string()).unwrap_or_default();
    for m in &mons {
        let name = s(m, &["name"]).to_string();
        let enabled = m["enabled"] == true;
        let mirror = s(m, &["mirrorOf"]);
        let note = if !enabled {
            "off".to_string()
        } else if !mirror.is_empty() {
            format!("mirrors {mirror}")
        } else {
            format!("{}×{} ×{}", m["width"], m["height"], m["scale"].as_f64().unwrap_or(1.0))
        };
        let desc = s(m, &["description"]);
        let title = if desc.is_empty() { name.clone() } else { format!("{desc} ({name})") };
        let n = name.clone();
        items.append(&row("video-display-symbolic", &title, &note, enabled, move || {
            service(&["displays", if enabled { "off" } else { "on" }, &n]);
        }));
        if !enabled {
            continue;
        }
        let (mode, scale) = (s(m, &["mode"]).to_string(), m["scale"].as_f64().unwrap_or(1.0));
        let pos = format!("{}x{}", m["x"], m["y"]);

        // the modes: a dropdown from Hyprland's list, the mode now picked
        let modes = m["availableModes"].as_array().into_iter().flatten().filter_map(Value::as_str);
        let modes: Vec<String> = modes.map(String::from).collect();
        if modes.len() > 1 {
            let strs: Vec<&str> = modes.iter().map(String::as_str).collect();
            let dd = gtk4::DropDown::from_strings(&strs);
            dd.set_margin_start(36);
            dd.set_margin_bottom(4);
            if let Some(i) = modes.iter().position(|x| *x == mode) {
                dd.set_selected(i as u32);
            }
            let (n, p) = (name.clone(), pos.clone());
            dd.connect_selected_notify(move |dd| {
                if let Some(new) = modes.get(dd.selected() as usize) {
                    service(&["displays", "set", &n, new, &p, &scale.to_string()]);
                }
            });
            items.append(&dd);
        }

        let names: Vec<String> = SCALES.iter().map(|x| x.to_string()).collect();
        let strs: Vec<&str> = names.iter().map(String::as_str).collect();
        let (n, md, p) = (name.clone(), mode.clone(), pos.clone());
        let cur = SCALES.iter().position(|x| (x - scale).abs() < 0.01);
        items.append(&chips(&strs, cur, move |i| service(&["displays", "set", &n, &md, &p, &SCALES[i].to_string()])));

        // beside the main one, or mirroring it
        if name != main && !main.is_empty() {
            let (n, md, main) = (name.clone(), mode.clone(), main.clone());
            let on = (!mirror.is_empty()).then_some(4);
            items.append(&chips(&["Left", "Right", "Above", "Below", "Mirror"], on, move |i| match i {
                4 => service(&["displays", "mirror", &n, &main]),
                _ => service(&["displays", "set", &n, &md, ["left", "right", "above", "below"][i], &scale.to_string()]),
            }));
        }
    }

    // the profiles
    items.append(&gtk4::Separator::new(Orientation::Horizontal));
    for p in d["profiles"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        let bx = gtk4::Box::new(Orientation::Horizontal, 0);
        let (p1, p2) = (p.to_string(), p.to_string());
        let r = row("", p, "", p == profile, move || service(&["displays", "load", &p1]));
        r.set_hexpand(true);
        bx.append(&r);
        let del = gtk4::Button::from_icon_name("user-trash-symbolic");
        del.add_css_class("flat-round");
        del.set_tooltip_text(Some("Delete"));
        del.connect_clicked(move |_| service(&["displays", "delete", &p2]));
        bx.append(&del);
        items.append(&bx);
    }
    let save = gtk4::Entry::new();
    save.set_placeholder_text(Some("Save as…"));
    save.set_margin_top(4);
    let note = label("", "error");
    note.set_visible(false);
    let n2 = note.clone();
    save.connect_activate(move |e| {
        let name = e.text().trim().to_string();
        if name.is_empty() {
            return;
        }
        let (e, note) = (e.clone(), n2.clone());
        service_then(vec!["displays".into(), "save".into(), name], None, move |r| match r {
            Ok(()) => e.set_text(""),
            Err(err) => {
                note.set_text(&err);
                note.set_visible(true);
            }
        });
    });
    items.append(&save);
    items.append(&note);
}

pub fn displays(c: &Ctx) -> Widget {
    let t = Toggle::new("video-display-symbolic", "Displays", {
        let flip = c.flip.clone();
        move || flip()
    }, Some(c.flip.clone()));
    let (card, items) = menu("video-display-symbolic", "Displays");
    let memo = Memo::default();
    let t2 = t.clone();
    Widget::toggle(&t, Some(&card), move |st| {
        let d = &st["displays"];
        draw(d, &t2, &items, memo.changed("displays", d.to_string()));
    })
}
