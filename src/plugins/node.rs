//! A plugin's widget as it declares it: a tree of nodes in JSON, each one of ui.rs's kit (a toggle, a slider, a
//! row...), drawn here into GTK's widgets. A node with an "id" says what happens to it (a click, a toggle, a
//! change) through emit. A tree like the last but in its values (a toggle's on and subtitle, a slider's value, a
//! label's text, a progress) updates the widgets drawn in place: a slider being dragged, an entry being typed
//! into, stay as they are. Fields left out take their defaults, unknown fields are ignored, a node of an unknown
//! type is drawn as nothing: a plugin written for a later ostrov still draws what this one knows.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::Orientation;
use serde::Deserialize;

use crate::style::{clear, label};
use crate::ui::{chips, round, row, Slider, Toggle};

#[derive(Deserialize, Clone, Debug, PartialEq, Default)]
pub struct El {
    #[serde(default)]
    pub id: String,
    #[serde(flatten)]
    pub kind: Kind,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    Toggle {
        #[serde(default)]
        icon: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        sub: String,
        #[serde(default)]
        on: bool,
        /// what its arrow unfolds, under the grid's rows (the widget's root toggle's alone)
        #[serde(default)]
        menu: Option<Box<El>>,
    },
    Slider {
        #[serde(default)]
        icon: String,
        #[serde(default)]
        value: f64,
    },
    Button {
        #[serde(default)]
        icon: String,
        #[serde(default)]
        label: String,
    },
    Round {
        #[serde(default)]
        icon: String,
        #[serde(default)]
        label: String,
    },
    Label {
        #[serde(default)]
        text: String,
        #[serde(default)]
        class: String,
    },
    Row {
        #[serde(default)]
        icon: String,
        #[serde(default)]
        text: String,
        #[serde(default)]
        note: String,
        #[serde(default)]
        on: bool,
    },
    Box {
        #[serde(default)]
        orientation: String,
        #[serde(default)]
        children: Vec<El>,
    },
    Image {
        #[serde(default)]
        icon: String,
        /// a file's absolute path
        #[serde(default)]
        path: String,
    },
    Progress {
        #[serde(default)]
        value: f64,
    },
    Chips {
        #[serde(default)]
        options: Vec<String>,
        #[serde(default)]
        on: Option<usize>,
    },
    Entry {
        #[serde(default)]
        placeholder: String,
    },
    Separator,
    #[default]
    #[serde(other)]
    Unknown,
}

impl El {
    /// The tree without the values updated in place: trees of one shape are drawn with the same widgets. A
    /// toggle's menu, drawn on its own, counts only as being there or not.
    pub fn shape(&self) -> El {
        let mut e = self.clone();
        match &mut e.kind {
            Kind::Toggle { icon, sub, on, menu, .. } => {
                icon.clear();
                sub.clear();
                *on = false;
                if let Some(m) = menu {
                    **m = El::default();
                }
            }
            Kind::Slider { icon, value } => {
                icon.clear();
                *value = 0.0;
            }
            Kind::Label { text, .. } => text.clear(),
            Kind::Progress { value } => *value = 0.0,
            Kind::Box { children, .. } => children.iter_mut().for_each(|c| *c = c.shape()),
            _ => {}
        }
        e
    }

    /// The tree's nodes in the order they are drawn, the root first.
    fn walk<'a>(&'a self, out: &mut Vec<&'a El>) {
        out.push(self);
        if let Kind::Box { children, .. } = &self.kind {
            children.iter().for_each(|c| c.walk(out));
        }
    }

    /// The menu its arrow unfolds, if it is a toggle with one.
    pub fn menu(&self) -> Option<&El> {
        match &self.kind {
            Kind::Toggle { menu, .. } => menu.as_deref(),
            _ => None,
        }
    }
}

/// What happens to a node: its id, the event (click, toggle, change), its value as text (a string in the wit).
pub type Emit = Rc<dyn Fn(&str, &str, String)>;
type Update = Box<dyn Fn(&El)>;

/// A tree as drawn: its shape, how each of its nodes takes new values (in walk's order), its toggles to fit.
pub struct Drawn {
    shape: El,
    updates: Vec<Update>,
    pub toggles: Vec<Toggle>,
}

/// el drawn into the box; one of the shape last drawn there updates its widgets instead. flip, the arrow's,
/// goes to the root's toggle if it has a menu.
pub fn draw(into: &gtk4::Box, el: &El, last: &mut Option<Drawn>, emit: &Emit, flip: Option<Rc<dyn Fn()>>) {
    let shape = el.shape();
    if let Some(d) = last.as_ref().filter(|d| d.shape == shape) {
        let mut nodes = Vec::new();
        el.walk(&mut nodes);
        d.updates.iter().zip(nodes).for_each(|(u, n)| u(n));
        return;
    }
    clear(into);
    let mut d = Drawn { shape, updates: Vec::new(), toggles: Vec::new() };
    into.append(&build(el, emit, flip, &mut d));
    *last = Some(d);
}

fn build(el: &El, emit: &Emit, flip: Option<Rc<dyn Fn()>>, d: &mut Drawn) -> gtk4::Widget {
    // this node's update before its children's, walk's order
    let at = d.updates.len();
    d.updates.push(Box::new(|_| ()));
    let em = {
        let (e, id) = (emit.clone(), el.id.clone());
        move |ev: &str, v: String| e(&id, ev, v)
    };
    let (w, update): (gtk4::Widget, Update) = match &el.kind {
        Kind::Toggle { icon, title, sub, on, menu } => {
            let cur = Rc::new(Cell::new(*on));
            let c = cur.clone();
            let flip = flip.filter(|_| menu.is_some());
            let t = Toggle::new(icon, title, move || em("toggle", (!c.get()).to_string()), flip);
            t.set(*on, icon, sub);
            d.toggles.push(t.clone());
            let t2 = t.clone();
            let up = move |n: &El| {
                if let Kind::Toggle { icon, sub, on, .. } = &n.kind {
                    cur.set(*on);
                    t2.set(*on, icon, sub);
                }
            };
            (t.root.upcast(), Box::new(up))
        }
        Kind::Slider { icon, value } => {
            let sl = Rc::new(Slider::new(icon, move |v| em("change", v.to_string())));
            sl.set(*value, icon);
            let s2 = sl.clone();
            let up = move |n: &El| {
                if let Kind::Slider { icon, value } = &n.kind {
                    s2.set(*value, icon);
                }
            };
            (sl.root.clone().upcast(), Box::new(up))
        }
        Kind::Button { icon, label: text } => {
            let b = gtk4::Button::new();
            b.add_css_class("chip");
            let bx = gtk4::Box::new(Orientation::Horizontal, 6);
            if !icon.is_empty() {
                bx.append(&gtk4::Image::from_icon_name(icon));
            }
            if !text.is_empty() {
                bx.append(&label(text, ""));
            }
            b.set_child(Some(&bx));
            b.connect_clicked(move |_| em("click", String::new()));
            (b.upcast(), Box::new(|_| ()))
        }
        Kind::Round { icon, label } => {
            let b = round(icon, move || em("click", String::new()));
            if !label.is_empty() {
                b.set_tooltip_text(Some(label));
            }
            (b.upcast(), Box::new(|_| ()))
        }
        Kind::Label { text, class } => {
            let l = label(text, class);
            l.set_wrap(true);
            let l2 = l.clone();
            let up = move |n: &El| {
                if let Kind::Label { text, .. } = &n.kind {
                    l2.set_text(text);
                }
            };
            (l.upcast(), Box::new(up))
        }
        Kind::Row { icon, text, note, on } => {
            (row(icon, text, note, *on, move || em("click", String::new())).upcast(), Box::new(|_| ()))
        }
        Kind::Box { orientation, children } => {
            let o = if orientation == "horizontal" { Orientation::Horizontal } else { Orientation::Vertical };
            let bx = gtk4::Box::new(o, 6);
            bx.set_valign(gtk4::Align::Center);
            for c in children {
                bx.append(&build(c, emit, None, d));
            }
            (bx.upcast(), Box::new(|_| ()))
        }
        Kind::Image { icon, path } => {
            let img = if path.is_empty() { gtk4::Image::from_icon_name(icon) } else { gtk4::Image::from_file(path) };
            (img.upcast(), Box::new(|_| ()))
        }
        Kind::Progress { value } => {
            let p = gtk4::ProgressBar::new();
            p.add_css_class("progress");
            p.set_hexpand(true);
            p.set_valign(gtk4::Align::Center);
            p.set_fraction(value.clamp(0.0, 1.0));
            let p2 = p.clone();
            let up = move |n: &El| {
                if let Kind::Progress { value } = &n.kind {
                    p2.set_fraction(value.clamp(0.0, 1.0));
                }
            };
            (p.upcast(), Box::new(up))
        }
        Kind::Chips { options, on } => {
            let names: Vec<&str> = options.iter().map(String::as_str).collect();
            let em = Rc::new(em);
            (chips(&names, *on, move |i| em("change", i.to_string())).upcast(), Box::new(|_| ()))
        }
        Kind::Entry { placeholder } => {
            let e = gtk4::Entry::new();
            e.set_placeholder_text(Some(placeholder));
            e.set_hexpand(true);
            e.connect_activate(move |e| em("change", e.text().to_string()));
            (e.upcast(), Box::new(|_| ()))
        }
        Kind::Separator => (gtk4::Separator::new(Orientation::Horizontal).upcast(), Box::new(|_| ())),
        Kind::Unknown => (gtk4::Box::new(Orientation::Horizontal, 0).upcast(), Box::new(|_| ())),
    };
    d.updates[at] = update;
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn el(v: Value) -> El {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn parses_every_kind() {
        let t = el(json!({
            "type": "toggle", "id": "t", "icon": "a-symbolic", "title": "Hello", "on": true,
            "menu": {"type": "box", "children": [
                {"type": "row", "id": "r", "text": "one", "on": true},
                {"type": "slider", "id": "s", "value": 1},
                {"type": "chips", "options": ["a", "b"], "on": 1},
                {"type": "entry", "placeholder": "type"},
                {"type": "progress", "value": 0.5},
                {"type": "image", "path": "/x.png"},
                {"type": "button", "label": "Go"},
                {"type": "round", "icon": "b"},
                {"type": "label", "text": "hi", "class": "dim"},
                {"type": "separator"}
            ]}
        }));
        assert_eq!(t.id, "t");
        let Kind::Toggle { title, on, sub, .. } = &t.kind else { panic!("{t:?}") };
        assert_eq!((title.as_str(), *on, sub.as_str()), ("Hello", true, ""));
        let Some(El { kind: Kind::Box { children, orientation }, .. }) = t.menu() else { panic!() };
        assert_eq!(orientation, "");
        assert_eq!(children.len(), 10);
        assert_eq!(children[1].kind, Kind::Slider { icon: String::new(), value: 1.0 });
        assert_eq!(children[2].kind, Kind::Chips { options: vec!["a".into(), "b".into()], on: Some(1) });
    }

    #[test]
    fn unknown_types_and_fields_are_tolerated() {
        let e = el(json!({"type": "hologram", "id": "h", "depth": 3}));
        assert_eq!(e, El { id: "h".into(), kind: Kind::Unknown });
        let e = el(json!({"type": "label", "text": "x", "font": "big"}));
        assert_eq!(e.kind, Kind::Label { text: "x".into(), class: String::new() });
        assert!(serde_json::from_value::<El>(json!({"text": "no type"})).is_err());
        assert!(serde_json::from_value::<El>(json!({"type": "slider", "value": "loud"})).is_err());
    }

    #[test]
    fn shape_ignores_values_alone() {
        let a = el(json!({"type": "box", "children": [
            {"type": "toggle", "title": "T", "on": false, "menu": {"type": "label", "text": "1"}},
            {"type": "slider", "value": 0.2}, {"type": "label", "text": "a"}
        ]}));
        let b = el(json!({"type": "box", "children": [
            {"type": "toggle", "title": "T", "on": true, "sub": "on", "menu": {"type": "row", "text": "2"}},
            {"type": "slider", "value": 0.9}, {"type": "label", "text": "b"}
        ]}));
        assert_eq!(a.shape(), b.shape());
        // a title, a row's tick, a child more: drawn anew
        let c = el(json!({"type": "box", "children": [{"type": "toggle", "title": "U"}]}));
        assert_ne!(a.shape(), c.shape());
        let r1 = el(json!({"type": "row", "text": "x"}));
        let r2 = el(json!({"type": "row", "text": "x", "on": true}));
        assert_ne!(r1.shape(), r2.shape());
        let mut n = Vec::new();
        a.walk(&mut n);
        assert_eq!(n.len(), 4);
    }
}
