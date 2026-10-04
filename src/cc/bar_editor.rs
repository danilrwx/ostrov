//! The bar's editor, a page of the control centre's Settings: the bar's three parts, left, middle and right, each
//! a row of its blocks' chips. A chip picked moves along its row with the arrows or to another part, or goes;
//! under them the blocks not in the bar, a click putting one at the end of the part the picked chip is in (else
//! the right), and a name making a new panel ([panels.ID], empty until its Edit fills it). Apply writes [bar] in
//! the config and starts ostrov again on it (ostrov restart).

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Align, Orientation};
use serde_json::{json, Value};

use crate::style::{clear, label};

/// The blocks there are besides panels.
const BLOCKS: &[(&str, &str, &str)] = &[
    ("workspaces", "Workspaces", "view-grid-symbolic"),
    ("window", "Window Title", "window-new-symbolic"),
    ("layout", "Keyboard Layout", "input-keyboard-symbolic"),
    ("tray", "Tray", "application-x-addon-symbolic"),
    ("privacy", "Privacy", "camera-web-symbolic"),
    ("record", "Recording", "media-record-symbolic"),
];

const PARTS: [(&str, &str); 3] = [("left", "Left"), ("center", "Middle"), ("right", "Right")];

/// What the editor holds while it edits: the three parts' names, the chip picked (part, index).
struct State {
    parts: [Vec<String>; 3],
    picked: Option<(usize, usize)>,
}

/// A block's name and icon as the editor shows it.
fn named(id: &str) -> (String, String) {
    let id = match id {
        "status" => "panel.control",
        "clock" => "panel.calendar",
        n => n,
    };
    if let Some(w) = id.strip_prefix("widget.") {
        let reg = super::registry();
        return reg.iter().find(|m| m.id == w).map_or((w.into(), "image-missing-symbolic".into()), |m| (m.name.into(), m.icon.into()));
    }
    if let Some(p) = id.strip_prefix("panel.") {
        let spec = super::Spec::of(p);
        return (spec.name, spec.icon);
    }
    BLOCKS.iter().find(|b| b.0 == id).map_or((id.into(), "image-missing-symbolic".into()), |b| (b.1.into(), b.2.into()))
}

pub fn page(back: impl Fn() + 'static) -> gtk4::Box {
    let root = gtk4::Box::new(Orientation::Vertical, 8);
    let (head, _) = crate::ui::header("Bar", back);
    root.append(&head);
    let body = gtk4::Box::new(Orientation::Vertical, 8);
    root.append(&body);
    let cfg = crate::config::load();
    let st = Rc::new(RefCell::new(State { parts: [cfg.bar.left, cfg.bar.center, cfg.bar.right], picked: None }));
    let redraw: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::default();
    let again = {
        let r = redraw.clone();
        move || {
            if let Some(f) = r.borrow().clone() {
                f()
            }
        }
    };
    let draw: Rc<dyn Fn()> = {
        let (st, body, again) = (st.clone(), body.clone(), again.clone());
        Rc::new(move || {
            clear(&body);
            draw(&body, &st, Rc::new(again.clone()));
        })
    };
    *redraw.borrow_mut() = Some(draw.clone());
    // anew on every showing: the config as it is now, nothing picked
    let (st2, d2) = (st.clone(), draw.clone());
    root.connect_map(move |_| {
        let cfg = crate::config::load();
        *st2.borrow_mut() = State { parts: [cfg.bar.left, cfg.bar.center, cfg.bar.right], picked: None };
        d2();
    });
    draw();
    root
}

fn draw(body: &gtk4::Box, st: &Rc<RefCell<State>>, again: Rc<dyn Fn()>) {
    let s = st.borrow();
    for (i, (_, title)) in PARTS.iter().enumerate() {
        body.append(&label(title, "dim"));
        let row = crate::ui::chip_flow();
        row.add_css_class("bar-part");
        for (j, id) in s.parts[i].iter().enumerate() {
            let (name, icon) = named(id);
            let chip = gtk4::ToggleButton::new();
            chip.add_css_class("chip");
            let bx = gtk4::Box::new(Orientation::Horizontal, 6);
            bx.append(&gtk4::Image::from_icon_name(&icon));
            bx.append(&gtk4::Label::new(Some(&name)));
            chip.set_child(Some(&bx));
            chip.set_active(s.picked == Some((i, j)));
            let (st, again) = (st.clone(), again.clone());
            chip.connect_clicked(move |_| {
                let now = if st.borrow().picked == Some((i, j)) { None } else { Some((i, j)) };
                st.borrow_mut().picked = now;
                again();
            });
            crate::ui::flow_in(&row, &chip);
        }
        if s.parts[i].is_empty() {
            crate::ui::flow_in(&row, &label("empty", "dim"));
        }
        body.append(&row);
    }

    // what the picked chip can do
    if let Some((i, j)) = s.picked {
        let acts = gtk4::Box::new(Orientation::Horizontal, 4);
        let act = |icon: &str, tip: &str, f: Box<dyn Fn(&mut State)>| {
            let b = gtk4::Button::from_icon_name(icon);
            b.add_css_class("flat-round");
            b.set_tooltip_text(Some(tip));
            let (st, again) = (st.clone(), again.clone());
            b.connect_clicked(move |_| {
                f(&mut st.borrow_mut());
                again();
            });
            acts.append(&b);
        };
        act("go-previous-symbolic", "Earlier", Box::new(move |s| {
            if j > 0 {
                s.parts[i].swap(j, j - 1);
                s.picked = Some((i, j - 1));
            }
        }));
        act("go-next-symbolic", "Later", Box::new(move |s| {
            if j + 1 < s.parts[i].len() {
                s.parts[i].swap(j, j + 1);
                s.picked = Some((i, j + 1));
            }
        }));
        for (k, (_, title)) in PARTS.iter().enumerate().filter(|(k, _)| *k != i) {
            let b = gtk4::Button::with_label(&format!("To {title}"));
            b.add_css_class("chip");
            let (st, again) = (st.clone(), again.clone());
            b.connect_clicked(move |_| {
                let mut s = st.borrow_mut();
                let id = s.parts[i].remove(j);
                s.parts[k].push(id);
                s.picked = Some((k, s.parts[k].len() - 1));
                drop(s);
                again();
            });
            acts.append(&b);
        }
        act("list-remove-symbolic", "Take off the bar", Box::new(move |s| {
            s.parts[i].remove(j);
            s.picked = None;
        }));
        body.append(&acts);
    }

    // the blocks not in the bar, and the panels
    let cfg = crate::config::load();
    let mut all: Vec<String> = BLOCKS.iter().map(|b| b.0.to_string()).collect();
    all.extend(["panel.control", "panel.calendar"].map(String::from));
    all.extend(cfg.panels.keys().filter(|k| *k != "control" && *k != "calendar").map(|k| format!("panel.{k}")));
    // every widget on its own, its badge the block
    all.extend(super::registry().iter().map(|m| format!("widget.{}", m.id)));
    let placed = |id: &str| s.parts.iter().flatten().any(|p| p == id || named(p).0 == named(id).0);
    let missing: Vec<String> = all.into_iter().filter(|id| !placed(id)).collect();
    let into = s.picked.map_or(2, |(i, _)| i);
    drop(s);
    if !missing.is_empty() {
        body.append(&label("Add to the Bar", "title"));
    }
    for id in missing {
        let (name, icon) = named(&id);
        let (st, again) = (st.clone(), again.clone());
        body.append(&crate::ui::row(&icon, &name, PARTS[into].1, false, move || {
            st.borrow_mut().parts[into].push(id.clone());
            again();
        }));
    }

    // a new panel: its name, its id made of it
    let new = gtk4::Entry::new();
    new.set_placeholder_text(Some("New panel's name…"));
    let note = label("", "error");
    note.set_visible(false);
    let (st2, again2, n2) = (st.clone(), again.clone(), note.clone());
    new.connect_activate(move |e| {
        let name = e.text().trim().to_string();
        let id: String = name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
        let id = id.trim_matches('-').to_string();
        if id.is_empty() || id == "control" || id == "calendar" {
            return;
        }
        match crate::settings::write(&format!("panels.{id}"), "name", Some(&json!(name)), false) {
            Ok(()) => {
                st2.borrow_mut().parts[into].push(format!("panel.{id}"));
                again2();
            }
            Err(e) => {
                n2.set_text(&e);
                n2.set_visible(true);
            }
        }
    });
    body.append(&new);
    body.append(&note);

    let apply = gtk4::Button::with_label("Apply");
    apply.add_css_class("connect");
    apply.set_halign(Align::End);
    let st = st.clone();
    apply.connect_clicked(move |_| {
        let s = st.borrow();
        for (i, (key, _)) in PARTS.iter().enumerate() {
            let v = Value::Array(s.parts[i].iter().map(|p| json!(p)).collect());
            if let Err(e) = crate::settings::write("bar", key, Some(&v), false) {
                eprintln!("ostrov: bar: {e}");
                return;
            }
        }
        crate::restart();
    });
    body.append(&apply);
}
