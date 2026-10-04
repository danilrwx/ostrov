//! ostrov's kit of things on a surface, what the control centre's widgets are made of (and plugins', declared):
//! a toggle, a slider, a round button, a menu's card and its rows, chips. Each looks the same wherever it goes;
//! the toggle and the round button fit the size of the cell they are put in.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use serde_json::Value;

use crate::hub::s;
use crate::style::label;

/// A menu row: an icon, its text, a note, a tick while on (the row inverted); a click picks it.
pub fn row(icon: &str, text: &str, note: &str, on: bool, pick: impl Fn() + 'static) -> gtk4::Button {
    let b = gtk4::Button::new();
    b.add_css_class("item");
    if on {
        b.add_css_class("on");
    }
    let bx = gtk4::Box::new(Orientation::Horizontal, 10);
    if !icon.is_empty() {
        bx.append(&gtk4::Image::from_icon_name(icon));
    }
    let t = label(text, "");
    t.set_hexpand(true);
    t.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    bx.append(&t);
    if !note.is_empty() {
        bx.append(&label(note, "dim"));
    }
    if on {
        bx.append(&gtk4::Image::from_icon_name("object-select-symbolic"));
    }
    b.set_child(Some(&bx));
    b.connect_clicked(move |_| pick());
    b
}

/// A line of chips, the one at on pressed, each running pick with its index.
pub fn chips(names: &[&str], on: Option<usize>, pick: impl Fn(usize) + Clone + 'static) -> gtk4::Box {
    let bx = gtk4::Box::new(Orientation::Horizontal, 4);
    bx.set_margin_start(36);
    bx.set_margin_bottom(4);
    for (i, n) in names.iter().enumerate() {
        let b = gtk4::ToggleButton::with_label(n);
        b.add_css_class("chip");
        b.set_active(on == Some(i));
        let p = pick.clone();
        b.connect_clicked(move |_| p(i));
        bx.append(&b);
    }
    bx
}

/// A right click on a widget runs f.
pub fn on_right_click(w: &impl IsA<gtk4::Widget>, f: impl Fn() + 'static) {
    let g = gtk4::GestureClick::new();
    g.set_button(3);
    g.connect_released(move |_, _, _, _| f());
    w.add_controller(g);
}

/// A menu's card: a header (its icon on a white square, its title) over a box its items go into.
pub fn menu(icon: &str, title: &str) -> (gtk4::Box, gtk4::Box) {
    let card = gtk4::Box::new(Orientation::Vertical, 2);
    card.add_css_class("menu");
    let head = gtk4::Box::new(Orientation::Horizontal, 10);
    head.add_css_class("menu-head");
    let badge = gtk4::Image::from_icon_name(icon);
    badge.add_css_class("badge");
    head.append(&badge);
    head.append(&label(title, "title"));
    card.append(&head);
    let items = gtk4::Box::new(Orientation::Vertical, 0);
    card.append(&items);
    (card, items)
}

/// The arrow that unfolds a widget's menu, turned while it is open (its tile's "open").
pub fn arrow(flip: Rc<dyn Fn()>) -> gtk4::Button {
    let b = gtk4::Button::from_icon_name("go-next-symbolic");
    b.add_css_class("arrow");
    b.connect_clicked(move |_| flip());
    b
}

/// A round button filling its cell, its icon in the middle.
pub fn round(icon: &str, f: impl Fn() + 'static) -> gtk4::Button {
    let b = gtk4::Button::from_icon_name(icon);
    b.add_css_class("round");
    b.set_hexpand(true);
    b.set_vexpand(true);
    b.connect_clicked(move |_| f());
    b
}

/// A split toggle: the pill flips it (inverted while on), the arrow side (or a right click) opens its menu.
/// Narrower than four cells it is its icon alone.
#[derive(Clone)]
pub struct Toggle {
    pub root: gtk4::Box,
    icon: gtk4::Image,
    inner: gtk4::Box,
    col: gtk4::Box,
    sub: gtk4::Label,
    side: Option<gtk4::Button>,
}

impl Toggle {
    pub fn new(icon: &str, title: &str, flip: impl Fn() + 'static, menu: Option<Rc<dyn Fn()>>) -> Toggle {
        let root = gtk4::Box::new(Orientation::Horizontal, 0);
        root.add_css_class("toggle");
        root.set_hexpand(true);
        let main = gtk4::Button::new();
        main.add_css_class("toggle-main");
        main.set_hexpand(true);
        let bx = gtk4::Box::new(Orientation::Horizontal, 10);
        let img = gtk4::Image::from_icon_name(icon);
        bx.append(&img);
        let col = gtk4::Box::new(Orientation::Vertical, 0);
        col.set_valign(Align::Center);
        let t = label(title, "toggle-title");
        t.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let sub = label("", "toggle-sub");
        sub.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        sub.set_max_width_chars(14);
        col.append(&t);
        col.append(&sub);
        bx.append(&col);
        main.set_child(Some(&bx));
        main.set_tooltip_text(Some(title));
        main.connect_clicked(move |_| flip());
        root.append(&main);
        let side = menu.map(|m| {
            let m2 = m.clone();
            on_right_click(&main, move || m2());
            let a = arrow(m);
            a.add_css_class("toggle-side");
            root.append(&a);
            a
        });
        Toggle { root, icon: img, inner: bx, col, sub, side }
    }

    pub fn set(&self, on: bool, icon: &str, sub: &str) {
        if on {
            self.root.add_css_class("on");
        } else {
            self.root.remove_css_class("on");
        }
        if !icon.is_empty() {
            self.icon.set_icon_name(Some(icon));
        }
        self.sub.set_text(sub);
        self.sub.set_visible(!sub.is_empty());
    }

    /// Fitted to w cells: its words and arrow from four on, its icon alone below.
    pub fn size(&self, w: u8, _h: u8) {
        let full = w >= 4;
        self.col.set_visible(full);
        self.inner.set_halign(if full { Align::Fill } else { Align::Center });
        if let Some(a) = &self.side {
            a.set_visible(full);
        }
        if full {
            self.root.remove_css_class("small");
        } else {
            self.root.add_css_class("small");
        }
    }
}

/// A slider: an icon button (mute), a scale that runs moved with its value as the user moves it, an arrow.
/// The state's value comes back in through set, but not while the user is at it.
pub struct Slider {
    pub root: gtk4::Box,
    pub icon: gtk4::Button,
    scale: gtk4::Scale,
    touched: Rc<RefCell<Instant>>,
}

impl Slider {
    pub fn new(icon: &str, moved: impl Fn(f64) + 'static) -> Slider {
        let root = gtk4::Box::new(Orientation::Horizontal, 6);
        root.add_css_class("slider");
        let ib = gtk4::Button::from_icon_name(icon);
        ib.add_css_class("flat-round");
        let scale = gtk4::Scale::with_range(Orientation::Horizontal, 0.0, 1.0, 0.01);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        let touched = Rc::new(RefCell::new(Instant::now() - Duration::from_secs(10)));
        let t = touched.clone();
        scale.connect_change_value(move |_, _, v| {
            *t.borrow_mut() = Instant::now();
            moved(v.clamp(0.0, 1.0));
            glib::Propagation::Proceed
        });
        root.append(&ib);
        root.append(&scale);
        Slider { root, icon: ib, scale, touched }
    }

    pub fn set(&self, v: f64, icon: &str) {
        if self.touched.borrow().elapsed() > Duration::from_millis(800) {
            self.scale.set_value(v);
        }
        if !icon.is_empty() {
            self.icon.set_icon_name(icon);
        }
    }
}

/// What each part of a widget was last drawn from, so a list is drawn again only when that changes.
#[derive(Default)]
pub struct Memo(RefCell<HashMap<&'static str, String>>);

impl Memo {
    pub fn changed(&self, key: &'static str, v: String) -> bool {
        let mut d = self.0.borrow_mut();
        if d.get(key) == Some(&v) {
            return false;
        }
        d.insert(key, v);
        true
    }
}

/// An app's icon: the one its stream names, else its desktop entry's (the one named as the app or running its
/// binary, else one whose id has the binary in it: chrome's is google-chrome), else a generic one.
pub fn app_icon(icon: &str, bin: &str, name: &str) -> gtk4::Image {
    if !icon.is_empty() {
        return gtk4::Image::from_icon_name(icon);
    }
    let all = gtk4::gio::AppInfo::all();
    let exact = |a: &&gtk4::gio::AppInfo| {
        a.display_name().eq_ignore_ascii_case(name)
            // the command line's program, not executable(): GLib's may be NULL, which its binding takes as never
            || !bin.is_empty()
                && a.commandline().is_some_and(|c| {
                    let c = c.to_string_lossy();
                    let prog = c.split_whitespace().next().unwrap_or("");
                    std::path::Path::new(prog).file_name() == Some(bin.as_ref())
                })
    };
    let near = |a: &&gtk4::gio::AppInfo| !bin.is_empty() && a.id().is_some_and(|id| id.contains(bin));
    match all.iter().find(exact).or_else(|| all.iter().find(near)).and_then(|a| a.icon()) {
        Some(g) => gtk4::Image::from_gicon(&g),
        None => gtk4::Image::from_icon_name("audio-x-generic-symbolic"),
    }
}

pub fn level_icon(kind: &str, v: f64, muted: bool) -> String {
    let lv = if muted || v == 0.0 {
        "muted"
    } else if v < 0.34 {
        "low"
    } else if v < 0.67 {
        "medium"
    } else {
        "high"
    };
    format!("{kind}-{lv}-symbolic")
}

fn duration(secs: i64) -> String {
    let (h, m) = (secs / 3600, (secs % 3600 + 30) / 60);
    if h > 0 { format!("{h} h {m:02} min") } else { format!("{m} min") }
}

pub fn battery_time(b: &Value) -> String {
    match s(b, &["state"]) {
        "charging" => match b["toFull"].as_i64().unwrap_or(0) {
            0 => "charging".into(),
            t => format!("full in {}", duration(t)),
        },
        "fully-charged" => "full".into(),
        "pending-charge" => "not charging".into(),
        _ => match b["toEmpty"].as_i64().unwrap_or(0) {
            0 => String::new(),
            t => format!("{} left", duration(t)),
        },
    }
}
