//! ostrov's kit of things on a surface, what the control centre's widgets are made of (and plugins', declared):
//! a toggle, a slider, a round button, a menu's card and its rows, chips; and what the Settings' forms are made
//! of: a page's header, a setting's title and help beside its control, options as chips. Each looks the same
//! wherever it goes; the toggle and the round button fit the size of the cell they are put in.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use serde_json::Value;

use crate::i18n::{fill, t};
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
        let n = label(note, "dim");
        // a long note (a widget's sizes in the gallery) cut rather than widening what it is in
        n.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        bx.append(&n);
    }
    if on {
        bx.append(&gtk4::Image::from_icon_name("object-select-symbolic"));
    }
    b.set_child(Some(&bx));
    b.connect_clicked(move |_| pick());
    b
}

/// A line of chips, the one at on pressed, each running pick with its index as it is pressed.
pub fn chips(names: &[&str], on: Option<usize>, pick: impl Fn(usize) + Clone + 'static) -> gtk4::FlowBox {
    let on: Vec<bool> = (0..names.len()).map(|i| on == Some(i)).collect();
    options(names, &on, false, move |i, _| pick(i))
}

/// A choice of options as chips wrapping onto more lines as they need: one of them at a time, or any of them
/// (multi); pick runs with an option's index and whether it is on now.
pub fn options(
    names: &[&str],
    on: &[bool],
    multi: bool,
    pick: impl Fn(usize, bool) + Clone + 'static,
) -> gtk4::FlowBox {
    let fb = chip_flow();
    let mut first: Option<gtk4::ToggleButton> = None;
    for (i, n) in names.iter().enumerate() {
        let b = gtk4::ToggleButton::with_label(n);
        b.add_css_class("chip");
        shorten(&b);
        if !multi {
            match &first {
                Some(f) => b.set_group(Some(f)),
                None => first = Some(b.clone()),
            }
        }
        b.set_active(on.get(i) == Some(&true));
        let p = pick.clone();
        // one at a time: the one turned on alone tells, not the one its group turned off
        b.connect_toggled(move |b| {
            if multi || b.is_active() {
                p(i, b.is_active())
            }
        });
        flow_in(&fb, &b);
    }
    fb
}

/// A small button, a choice or an action beside others (`.chip`).
pub fn chip(text: &str) -> gtk4::Button {
    let b = gtk4::Button::with_label(text);
    b.add_css_class("chip");
    shorten(&b);
    b
}

/// A menu's main action, on the accent (`.connect`).
pub fn primary(text: &str) -> gtk4::Button {
    let b = gtk4::Button::with_label(text);
    b.add_css_class("connect");
    shorten(&b);
    b
}

/// A button's words shortened (…) where it has less room than they need, not cut at its edge.
pub fn shorten(b: &impl IsA<gtk4::Button>) {
    if let Some(l) = b.as_ref().child().and_downcast::<gtk4::Label>() {
        l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    }
}

/// A page's header (the control centre's Appearance, Settings): the arrow back, its title.
pub fn header(title: &str, back: impl Fn() + 'static) -> (gtk4::Box, gtk4::Label) {
    let bx = gtk4::Box::new(Orientation::Horizontal, 6);
    let b = gtk4::Button::from_icon_name("go-previous-symbolic");
    b.add_css_class("flat-round");
    b.set_tooltip_text(Some(t("Back")));
    b.connect_clicked(move |_| back());
    let t = label(title, "page-title");
    t.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    bx.append(&b);
    bx.append(&t);
    (bx, t)
}

/// A setting: its title, its help under it, its control beside them, or under them across the width (wide).
pub fn setting(title: &str, help: &str, control: &impl IsA<gtk4::Widget>, wide: bool) -> gtk4::Box {
    let root = gtk4::Box::new(Orientation::Vertical, 6);
    root.add_css_class("field");
    let line = gtk4::Box::new(Orientation::Horizontal, 10);
    let words = gtk4::Box::new(Orientation::Vertical, 2);
    words.set_hexpand(true);
    words.set_valign(Align::Center);
    words.append(&label(title, ""));
    if !help.is_empty() {
        let h = label(help, "field-help");
        h.set_wrap(true);
        h.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        words.append(&h);
    }
    line.append(&words);
    root.append(&line);
    if wide {
        root.append(control);
    } else {
        control.set_valign(Align::Center);
        line.append(control);
    }
    root
}

/// A row of chips that wraps onto more lines, each its own width, a gap between them.
pub fn chip_flow() -> gtk4::FlowBox {
    let f = gtk4::FlowBox::new();
    f.set_selection_mode(gtk4::SelectionMode::None);
    f.set_homogeneous(false);
    f.set_column_spacing(4);
    f.set_row_spacing(4);
    f.set_max_children_per_line(32);
    // as wide as its chips, not spread over the line's width
    f.set_halign(Align::Start);
    f
}

/// A chip put in a chip_flow at its own width, not stretched over the line.
pub fn flow_in(f: &gtk4::FlowBox, chip: &impl IsA<gtk4::Widget>) {
    chip.set_halign(Align::Start);
    f.insert(chip, -1);
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

/// A row that folds what it holds away under it: its title, an arrow at its right turned down while open; the
/// kit's own row, so a panel's settings and the bar's look like the menus around them.
pub fn fold(title: &str, child: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let col = gtk4::Box::new(Orientation::Vertical, 0);
    let b = gtk4::Button::new();
    b.add_css_class("item");
    let bx = gtk4::Box::new(Orientation::Horizontal, 10);
    let t = label(title, "");
    t.set_hexpand(true);
    bx.append(&t);
    let arrow = gtk4::Image::from_icon_name("go-next-symbolic");
    bx.append(&arrow);
    b.set_child(Some(&bx));
    let r = gtk4::Revealer::new();
    r.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
    r.set_transition_duration(100);
    r.set_child(Some(child));
    let r2 = r.clone();
    b.connect_clicked(move |_| {
        let open = !r2.reveals_child();
        r2.set_reveal_child(open);
        arrow.set_icon_name(Some(if open { "go-down-symbolic" } else { "go-next-symbolic" }));
    });
    col.append(&b);
    col.append(&r);
    col
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
/// Narrower than four cells it is its icon alone. Its badge for the bar is its icon, active while it is on.
#[derive(Clone)]
pub struct Toggle {
    pub root: gtk4::Box,
    icon: gtk4::Image,
    pub face: gtk4::Image,
    pub active: Rc<std::cell::Cell<bool>>,
    title: String,
    inner: gtk4::Box,
    col: gtk4::Box,
    sub: gtk4::Label,
    side: Option<gtk4::Button>,
    /// a word or a flag in the icon's place (glyph), the icon hidden while it shows
    glyph: gtk4::Label,
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
        let glyph = label("", "toggle-glyph");
        glyph.set_visible(false);
        bx.append(&glyph);
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
        let face = gtk4::Image::from_icon_name(icon);
        face.set_tooltip_text(Some(title));
        let active = Rc::new(std::cell::Cell::new(false));
        Toggle { root, icon: img, face, active, title: title.into(), inner: bx, col, sub, side, glyph }
    }

    /// A word or a flag shown in the icon's place ("" the icon again): the keyboard layout's EN or 🇺🇸.
    pub fn glyph(&self, text: &str) {
        self.glyph.set_text(text);
        self.glyph.set_visible(!text.is_empty());
        self.icon.set_visible(text.is_empty());
    }

    pub fn set(&self, on: bool, icon: &str, sub: &str) {
        if on {
            self.root.add_css_class("on");
        } else {
            self.root.remove_css_class("on");
        }
        if !icon.is_empty() {
            self.icon.set_icon_name(Some(icon));
            self.face.set_icon_name(Some(icon));
        }
        self.active.set(on);
        let tip = if sub.is_empty() { self.title.clone() } else { format!("{}: {sub}", self.title) };
        self.face.set_tooltip_text(Some(&tip));
        // its main button's hover the same (it is under the pointer, not the toggle's box)
        if let Some(main) = self.root.first_child() {
            main.set_tooltip_text(Some(&tip));
        }
        self.sub.set_text(sub);
        self.sub.set_visible(!sub.is_empty());
    }

    /// Fitted to w cells: its words and arrow from four on, its icon alone below.
    pub fn size(&self, w: u8, _h: u8) {
        // its words from three cells, about 146 points: room for a title and a subtitle
        let full = w >= 3;
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

/// A slider: an icon button (mute; none for ""), a scale that runs moved with its value as the user moves it, an
/// arrow. The state's value comes back in through set, but not while the user is at it. Moved, its value shows in a
/// bubble over it (popup.rs); with few values (12 steps or fewer) a dot marks each and the scale keeps to them.
/// How its track looks is [appearance] sliders' (style.rs), the same for every slider.
pub struct Slider {
    pub root: gtk4::Box,
    pub icon: gtk4::Button,
    /// its badge for the bar: its icon
    pub face: gtk4::Image,
    scale: gtk4::Scale,
    touched: Rc<RefCell<Instant>>,
    /// its value as words: the bubble's, the value beside it (with_value)
    words: Rc<dyn Fn(f64) -> String>,
}

/// The most steps a slider marks with dots and keeps to.
const DOTS: f64 = 12.0;

impl Slider {
    pub fn new(icon: &str, moved: impl Fn(f64) + 'static) -> Slider {
        Slider::with_range(icon, 0.0, 1.0, 0.01, Rc::new(|v| format!("{}%", (v * 100.0).round())), moved)
    }

    /// From min to max by step, its value as words says.
    pub fn with_range(icon: &str, min: f64, max: f64, step: f64, words: Rc<dyn Fn(f64) -> String>, moved: impl Fn(f64) + 'static) -> Slider {
        let root = gtk4::Box::new(Orientation::Horizontal, 6);
        root.add_css_class("slider");
        let ib = gtk4::Button::from_icon_name(if icon.is_empty() { "image-missing-symbolic" } else { icon });
        ib.add_css_class("flat-round");
        ib.set_visible(!icon.is_empty());
        let scale = gtk4::Scale::with_range(Orientation::Horizontal, min, max, step);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        let span = max - min;
        let steps = if step > 0.0 { (span / step).round() } else { 0.0 };
        let dotted = (2.0..=DOTS).contains(&steps);
        // its value on a step (10, not 9.95; 0.85, not 0.8500000000000001)
        let on_step = move |v: f64| {
            let v = if step > 0.0 { min + ((v - min) / step).round() * step } else { v };
            ((v.clamp(min, max)) * 1e6).round() / 1e6
        };
        let touched = Rc::new(RefCell::new(Instant::now() - Duration::from_secs(10)));
        let hide: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
        let (t, w) = (touched.clone(), words.clone());
        scale.connect_change_value(move |sc, _, v| {
            *t.borrow_mut() = Instant::now();
            let v = on_step(v);
            sc.set_value(v);
            moved(v);
            // the bubble over its knob (where the value puts it along the track: GTK's knob node has no bounds of
            // its own to go by), gone a moment after the last move
            if let Some(trough) = sc.first_child() {
                let frac = if max > min { (v - min) / (max - min) } else { 0.0 };
                crate::popup::bubble(&trough, frac, Some(&w(v)));
                if let Some(id) = hide.borrow_mut().take() {
                    id.remove();
                }
                let (h, k) = (hide.clone(), trough.downgrade());
                *hide.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_millis(900), move || {
                    h.borrow_mut().take();
                    if let Some(k) = k.upgrade() {
                        crate::popup::bubble(&k, 0.0, None);
                    }
                }));
            }
            glib::Propagation::Stop
        });
        root.append(&ib);
        if dotted {
            // a dot on each step, drawn over the track where GTK lays it (the knob's travel): those under the
            // filled part in its ink, the rest dim
            let over = gtk4::Overlay::new();
            over.set_hexpand(true);
            over.set_child(Some(&scale));
            for filled in [true, false] {
                let dots = gtk4::DrawingArea::new();
                dots.add_css_class("slider-dots");
                if filled {
                    dots.add_css_class("filled");
                }
                dots.set_can_target(false);
                let sc = scale.downgrade();
                scale.connect_value_changed({
                    let d = dots.downgrade();
                    move |_| if let Some(d) = d.upgrade() { d.queue_draw() }
                });
                dots.set_draw_func(move |area, cr, _, _| {
                    let Some(sc) = sc.upgrade() else { return };
                    let (Some(trough), Some(k)) = (sc.first_child(), knob(&sc)) else { return };
                    let (Some(tb), Some(kb)) = (trough.compute_bounds(area), k.compute_bounds(area)) else { return };
                    let (kw, y) = (kb.width() as f64, (tb.y() + tb.height() / 2.0) as f64);
                    let r = (tb.height() as f64 / 2.0 - 3.0).clamp(1.5, 3.0);
                    let c = area.color();
                    cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
                    let at = sc.value();
                    for i in 0..=steps as i32 {
                        if (min + span * i as f64 / steps <= at + 1e-9) != filled {
                            continue;
                        }
                        // the ends' dots inside the track's rounding
                        let inset = (kw / 2.0).max(r + 3.0);
                        let x = tb.x() as f64 + inset + (tb.width() as f64 - 2.0 * inset) * i as f64 / steps;
                        cr.arc(x, y, r, 0.0, std::f64::consts::TAU);
                        let _ = cr.fill();
                    }
                });
                over.add_overlay(&dots);
            }
            root.append(&over);
        } else {
            root.append(&scale);
        }
        let face = gtk4::Image::from_icon_name(icon);
        Slider { root, icon: ib, face, scale, touched, words }
    }

    /// Its value in words beside it, always (the Settings' sliders).
    pub fn with_value(self) -> Slider {
        let l = label(&(self.words)(self.scale.value()), "dim");
        l.add_css_class("slider-value");
        self.root.append(&l);
        let w = self.words.clone();
        self.scale.connect_value_changed(move |s| l.set_text(&w(s.value())));
        self
    }

    pub fn set(&self, v: f64, icon: &str) {
        if self.touched.borrow().elapsed() > Duration::from_millis(800) {
            self.scale.set_value(v);
        }
        if !icon.is_empty() {
            self.icon.set_icon_name(icon);
            self.face.set_icon_name(Some(icon));
        }
        self.face.set_tooltip_text(Some(&(self.words)(v)));
    }
}

/// A scale's knob: GTK's slider node in its trough.
fn knob(scale: &gtk4::Scale) -> Option<gtk4::Widget> {
    let trough = scale.first_child()?;
    let mut c = trough.first_child();
    while let Some(w) = c {
        if w.css_name() == "slider" {
            return Some(w);
        }
        c = w.next_sibling();
    }
    None
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
    if h > 0 { fill(t("{} h {} min"), &[&h, &format!("{m:02}")]) } else { fill(t("{} min"), &[&m]) }
}

pub fn battery_time(b: &Value) -> String {
    match s(b, &["state"]) {
        "charging" => match b["toFull"].as_i64().unwrap_or(0) {
            0 => t("charging").into(),
            secs => fill(t("full in {}"), &[&duration(secs)]),
        },
        "fully-charged" => t("full").into(),
        "pending-charge" => t("not charging").into(),
        _ => match b["toEmpty"].as_i64().unwrap_or(0) {
            0 => String::new(),
            secs => fill(t("{} left"), &[&duration(secs)]),
        },
    }
}

/// The kit itself, every piece in its states on one page (`ostrov kit`): to see a change to the kit or a theme
/// everywhere at once, and the screenshots' check. Nothing on it does anything but show itself.
pub fn gallery(back: impl Fn() + 'static) -> gtk4::Box {
    let root = gtk4::Box::new(Orientation::Vertical, 6);
    root.append(&header(t("Kit"), back).0);
    let body = gtk4::Box::new(Orientation::Vertical, 8);
    body.add_css_class("page-body");
    let part = |title: &str, w: &gtk4::Widget| {
        body.append(&label(title, "dim"));
        body.append(w);
    };

    // toggles: on with its menu, off, its glyph, its icon alone
    let grid = gtk4::Grid::new();
    grid.set_column_spacing(6);
    grid.set_row_spacing(6);
    grid.set_column_homogeneous(true);
    let none: Option<Rc<dyn Fn()>> = None;
    let on = Toggle::new("network-wireless-symbolic", "Wi-Fi", || (), Some(Rc::new(|| ())));
    on.set(true, "network-wireless-symbolic", "Home");
    let off = Toggle::new("bluetooth-symbolic", "Bluetooth", || (), Some(Rc::new(|| ())));
    off.set(false, "bluetooth-disabled-symbolic", "Off");
    let glyph = Toggle::new("input-keyboard-symbolic", "Keyboard", || (), none.clone());
    glyph.glyph("EN");
    glyph.set(false, "", "English");
    let small = Toggle::new("weather-clear-night-symbolic", "Night", || (), none);
    small.set(true, "", "");
    small.size(1, 1);
    grid.attach(&on.root, 0, 0, 2, 1);
    grid.attach(&off.root, 2, 0, 2, 1);
    grid.attach(&glyph.root, 0, 1, 2, 1);
    grid.attach(&small.root, 2, 1, 1, 1);
    grid.attach(&round("system-lock-screen-symbolic", || ()), 3, 1, 1, 1);
    part(t("Toggles and round buttons"), grid.upcast_ref());

    // sliders: any value, a few steps with their dots, its value beside it
    let sliders = gtk4::Box::new(Orientation::Vertical, 6);
    let any = Slider::new("audio-volume-high-symbolic", |_| ());
    any.set(0.6, "");
    any.root.append(&arrow(Rc::new(|| ())));
    let words: Rc<dyn Fn(f64) -> String> = Rc::new(|v| format!("{}%", (v * 100.0).round()));
    let steps = Slider::with_range("display-brightness-symbolic", 0.0, 1.0, 0.25, words, |_| ());
    steps.set(0.5, "");
    let n: Rc<dyn Fn(f64) -> String> = Rc::new(|v| format!("{v:.0}"));
    let valued = Slider::with_range("", 0.0, 20.0, 1.0, n, |_| ()).with_value();
    valued.set(10.0, "");
    for s in [&any.root, &steps.root, &valued.root] {
        sliders.append(s);
    }
    part(t("Sliders"), sliders.upcast_ref());

    // chips: one of them, any of them; buttons beside
    let chips = gtk4::Box::new(Orientation::Vertical, 6);
    chips.append(&options(&["Compact", "Normal", "Comfortable"], &[false, true, false], false, |_, _| ()));
    chips.append(&options(&["Mon", "Tue", "Wed", "Thu", "Fri"], &[true, true, false, true, false], true, |_, _| ()));
    let buttons = chip_flow();
    flow_in(&buttons, &chip("Cancel"));
    flow_in(&buttons, &primary("Connect"));
    let dis = chip("Disabled");
    dis.set_sensitive(false);
    flow_in(&buttons, &dis);
    chips.append(&buttons);
    part(t("Chips and buttons"), chips.upcast_ref());

    // a menu's card: its rows, the one on ticked, a fold
    let (card, items) = menu("network-wireless-symbolic", "Wi-Fi");
    items.append(&row("network-wireless-signal-excellent-symbolic", "Home", "", true, || ()));
    items.append(&row("network-wireless-signal-good-symbolic", "Office", "secured", false, || ()));
    items.append(&gtk4::Separator::new(Orientation::Horizontal));
    items.append(&fold("More", &label("What a fold holds", "dim")));
    part(t("Menu"), card.upcast_ref());

    // a form's fields: a switch, a number, words, a choice
    let form = gtk4::Box::new(Orientation::Vertical, 0);
    let sw = gtk4::Switch::new();
    sw.set_active(true);
    form.append(&setting("A switch", "Its help under its title.", &sw, false));
    form.append(&setting("A number", "", &gtk4::SpinButton::with_range(0.0, 10.0, 1.0), false));
    let entry = gtk4::Entry::new();
    entry.set_placeholder_text(Some("Words"));
    form.append(&setting("Words", "Across the width.", &entry, true));
    part(t("Settings"), form.upcast_ref());

    // words and the rest: their classes, a card, a progress, a badge
    let words = gtk4::Box::new(Orientation::Vertical, 4);
    for (text, class) in [("A title", "title"), ("Bold", "bold"), ("Plain", ""), ("Dim, said second", "dim"), ("An error", "error")] {
        words.append(&label(text, class));
    }
    let progress = gtk4::ProgressBar::new();
    progress.add_css_class("progress");
    progress.set_fraction(0.4);
    words.append(&progress);
    let c = gtk4::Box::new(Orientation::Horizontal, 10);
    c.add_css_class("card");
    let badge = gtk4::Image::from_icon_name("audio-speakers-symbolic");
    badge.add_css_class("badge");
    c.append(&badge);
    c.append(&label("A card, its badge", ""));
    words.append(&c);
    part(t("Words and cards"), words.upcast_ref());

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk4::PolicyType::Never);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(640);
    scroll.set_child(Some(&body));
    root.append(&scroll);
    root
}
