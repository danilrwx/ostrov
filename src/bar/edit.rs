//! The bar's editor, in the bar itself: `ostrov bar edit`, or Bar on the Settings page. The bar's three parts
//! outlined, a gallery unrolled under the bar with the blocks not in it: the system's (workspaces, the window's
//! title, the layout, the tray, privacy), the panels and every widget (ostrov's, a KDL file's, a plugin's) by
//! itself. A block dragged along the bar or into another part moves there, one from the gallery goes where it is
//! dropped, one dragged off the bar leaves it; a click on the gallery's puts it after the block picked (else at the
//! right's end). The bar is laid out anew at every change, nothing restarted. From the keyboard: Tab picks a
//! block, Shift+arrows move it, Delete takes it off, Enter is Done, Escape Cancel. Done writes [bar] and lays out
//! every bar so; Cancel, Escape or a click away puts the bar back as it was.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk4::prelude::*;
use gtk4::{gdk, glib, Orientation};
use serde_json::{json, Value};

use super::Bar;
use crate::bars::Bars;
use crate::i18n::t;
use crate::popup::{Host, Popup, Side};
use crate::style::label;

/// The blocks there are besides panels and widgets: their names, titles, icons.
const BLOCKS: &[(&str, &str, &str)] = &[
    ("workspaces", "Workspaces", "view-grid-symbolic"),
    ("window", "Window Title", "window-new-symbolic"),
    ("layout", "Keyboard Layout", "input-keyboard-symbolic"),
    ("tray", "Tray", "application-x-addon-symbolic"),
    ("privacy", "Privacy", "camera-web-symbolic"),
];

/// The parts' keys in [bar].
const PARTS: [&str; 3] = ["left", "center", "right"];

/// How far the pointer goes before a press is a drag.
const SLOP: f64 = 4.0;

thread_local! {
    static BARS: RefCell<Option<Rc<Bars>>> = const { RefCell::new(None) };
    static EDIT: RefCell<Option<Rc<Edit>>> = const { RefCell::new(None) };
}

/// The bars the editor lays out, kept as ostrov starts.
pub fn init(bars: &Rc<Bars>) {
    BARS.with(|b| *b.borrow_mut() = Some(bars.clone()));
}

/// A block's title and icon as the gallery shows it.
fn named(id: &str) -> (String, String) {
    let id = match id {
        "status" => "panel.control",
        "clock" => "panel.calendar",
        n => n,
    };
    if let Some(w) = id.strip_prefix("widget.") {
        let reg = crate::cc::registry();
        let m = reg.iter().find(|m| m.id == w);
        return m.map_or((w.into(), "image-missing-symbolic".into()), |m| (t(m.name).into(), m.icon.into()));
    }
    if let Some(p) = id.strip_prefix("panel.") {
        let spec = crate::cc::Spec::of(p);
        return (spec.name, spec.icon);
    }
    let b = BLOCKS.iter().find(|b| b.0 == id);
    b.map_or((id.into(), "image-missing-symbolic".into()), |b| (t(b.1).into(), b.2.into()))
}

/// Every block there is: the system's, the panels, the widgets.
fn every() -> Vec<(&'static str, Vec<String>)> {
    let cfg = crate::config::load();
    let mut panels: Vec<String> = ["panel.control", "panel.calendar"].map(String::from).to_vec();
    panels.extend(cfg.panels.keys().filter(|k| *k != "control" && *k != "calendar").map(|k| format!("panel.{k}")));
    vec![
        ("System", BLOCKS.iter().map(|b| b.0.to_string()).collect()),
        ("Panels", panels),
        ("Widgets", crate::cc::registry().iter().map(|m| format!("widget.{}", m.id)).collect()),
    ]
}

/// The layout with a block taken out and, given a place (part, index among the others), put there.
fn moved(layout: &[Vec<String>; 3], name: &str, to: Option<(usize, usize)>) -> [Vec<String>; 3] {
    let mut l = layout.clone();
    for part in l.iter_mut() {
        part.retain(|n| n != name);
    }
    if let Some((p, i)) = to {
        let i = i.min(l[p].len());
        l[p].insert(i, name.to_string());
    }
    l
}

/// Where a block is: its part and index.
fn place(layout: &[Vec<String>; 3], name: &str) -> Option<(usize, usize)> {
    layout.iter().enumerate().find_map(|(p, v)| v.iter().position(|n| n == name).map(|i| (p, i)))
}

/// One step of a block along the bar, left or right: within its part, else to the next part's near end.
fn step(layout: &[Vec<String>; 3], name: &str, right: bool) -> Option<(usize, usize)> {
    let (p, i) = place(layout, name)?;
    let len = layout[p].len();
    Some(match (right, i) {
        (false, 0) if p > 0 => (p - 1, layout[p - 1].len()),
        (false, 0) => (p, 0),
        (false, i) => (p, i - 1),
        (true, i) if i + 1 < len => (p, i + 1),
        (true, _) if p < 2 => (p + 1, 0),
        (true, i) => (p, i),
    })
}

/// A press being dragged: the block, whether it is in the bar (else the gallery's), where it began.
struct Drag {
    name: Option<String>,
    from_bar: bool,
    moving: bool,
}

struct Edit {
    bars: Rc<Bars>,
    bar: Rc<Bar>,
    host: Rc<Host>,
    layout: RefCell<[Vec<String>; 3]>,
    was: [Vec<String>; 3],
    gallery: Rc<Popup>,
    shelf: gtk4::Box,
    marker: gtk4::Box,
    picked: RefCell<Option<String>>,
    drag: RefCell<Option<Drag>>,
    controllers: RefCell<Vec<gtk4::EventController>>,
    ending: Cell<bool>,
}

fn current() -> Option<Rc<Edit>> {
    EDIT.with(|e| e.borrow().clone())
}

/// The editor opened on the focused monitor's bar.
pub fn start() -> Result<(), String> {
    if current().is_some() {
        return Ok(());
    }
    let bars = BARS.with(|b| b.borrow().clone()).ok_or("ostrov is starting")?;
    let bar = bars.focused();
    let host = bar.host();
    if let Some(p) = host.popup() {
        p.close();
    }
    // shown while it is edited, a hidden bar too
    host.peeking.set(true);
    host.apply();

    let body = gtk4::Box::new(Orientation::Vertical, 12);
    body.add_css_class("surface");
    body.add_css_class("bar-gallery");
    let shelf = gtk4::Box::new(Orientation::Vertical, 10);
    body.append(&shelf);
    let foot = gtk4::Box::new(Orientation::Horizontal, 8);
    let hint = label(t("Drag a block into the bar, along it, or off it here; Tab, Shift+arrows and Delete too"), "dim");
    hint.set_hexpand(true);
    hint.set_wrap(true);
    foot.append(&hint);
    let cancel = gtk4::Button::with_label(t("Cancel"));
    cancel.add_css_class("chip");
    let done = gtk4::Button::with_label(t("Done"));
    done.add_css_class("connect");
    foot.append(&cancel);
    foot.append(&done);
    body.append(&foot);
    body.set_size_request(bar.strip.width().max(400), -1);
    let gallery = Popup::new(&host, &bar.strip, Side::Left, -1, &body);

    let marker = gtk4::Box::new(Orientation::Horizontal, 0);
    marker.add_css_class("drop-marker");
    let layout = bars.layout();
    let e = Rc::new(Edit {
        bars,
        bar,
        host,
        layout: RefCell::new(layout.clone()),
        was: layout,
        gallery,
        shelf,
        marker,
        picked: RefCell::default(),
        drag: RefCell::default(),
        controllers: RefCell::default(),
        ending: Cell::new(false),
    });
    cancel.connect_clicked(|_| finish(false));
    done.connect_clicked(|_| finish(true));
    // closed by a click away or Escape: as it was
    body.connect_unmap(|_| {
        if current().is_some_and(|e| !e.ending.get()) {
            finish(false);
        }
    });
    e.bar.strip.add_css_class("editing");
    for part in e.bar.parts() {
        part.add_css_class("zone");
    }
    e.listen();
    EDIT.with(|x| *x.borrow_mut() = Some(e.clone()));
    e.redraw();
    e.gallery.open();
    Ok(())
}

/// The editing ended: kept (written to the config, every bar laid out so) or put back.
fn finish(keep: bool) {
    let Some(e) = current() else { return };
    if e.ending.replace(true) {
        return;
    }
    let layout = if keep { e.layout.borrow().clone() } else { e.was.clone() };
    if keep {
        for (key, part) in PARTS.iter().zip(&layout) {
            let v = Value::Array(part.iter().map(|n| json!(n)).collect());
            if let Err(err) = crate::settings::write("bar", key, Some(&v), false) {
                eprintln!("ostrov: bar: {err}");
            }
        }
    }
    for c in e.controllers.take() {
        e.host.win.remove_controller(&c);
    }
    e.unmark();
    e.bar.strip.remove_css_class("editing");
    for part in e.bar.parts() {
        part.remove_css_class("zone");
    }
    for (_, w) in e.bar.widgets() {
        w.remove_css_class("edit-picked");
        w.set_opacity(1.0);
    }
    e.bars.relayout(layout);
    e.gallery.close();
    let g = e.gallery.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || g.remove());
    e.host.peeking.set(false);
    e.host.apply();
    EDIT.with(|x| *x.borrow_mut() = None);
}

impl Edit {
    /// The pointer and the keys, the bar's and the gallery's, the editor's while it lasts.
    fn listen(self: &Rc<Self>) {
        let drag = gtk4::GestureDrag::new();
        drag.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let me = Rc::downgrade(self);
        drag.connect_drag_begin(move |g, x, y| {
            let Some(e) = me.upgrade() else { return };
            match e.source(x, y) {
                Some(d) => {
                    g.set_state(gtk4::EventSequenceState::Claimed);
                    *e.drag.borrow_mut() = Some(d);
                }
                None => {
                    g.set_state(gtk4::EventSequenceState::Denied);
                }
            }
        });
        let me = Rc::downgrade(self);
        drag.connect_drag_update(move |g, dx, dy| {
            if let (Some(e), Some((x, y))) = (me.upgrade(), g.start_point()) {
                e.over(x + dx, y + dy, dx.hypot(dy));
            }
        });
        let me: Weak<Edit> = Rc::downgrade(self);
        drag.connect_drag_end(move |g, dx, dy| {
            if let (Some(e), Some((x, y))) = (me.upgrade(), g.start_point()) {
                e.dropped(x + dx, y + dy);
            }
        });
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let me = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, mods| {
            let Some(e) = me.upgrade() else { return glib::Propagation::Proceed };
            e.key(key, mods)
        });
        self.host.win.add_controller(drag.clone());
        self.host.win.add_controller(keys.clone());
        self.controllers.borrow_mut().extend([drag.upcast(), keys.upcast()]);
    }

    /// What a press at x, y in the window takes: a block of the bar's, a gallery's, or the bar itself (nothing,
    /// but held: a click on the bar does not close the gallery); None elsewhere, the gallery's buttons and entry.
    fn source(&self, x: f64, y: f64) -> Option<Drag> {
        if y < crate::popup::bar() as f64 {
            let name = self.block_at(x, y);
            return Some(Drag { name, from_bar: true, moving: false });
        }
        let mut w = self.host.win.pick(x, y, gtk4::PickFlags::DEFAULT);
        while let Some(x) = w {
            if x.has_css_class("gallery-item") {
                return Some(Drag { name: Some(x.widget_name().to_string()), from_bar: false, moving: false });
            }
            w = x.parent();
        }
        None
    }

    fn bounds(&self, w: &impl IsA<gtk4::Widget>) -> Option<gtk4::graphene::Rect> {
        w.compute_bounds(&self.host.win)
    }

    fn block_at(&self, x: f64, y: f64) -> Option<String> {
        let p = gtk4::graphene::Point::new(x as f32, y as f32);
        self.bar.widgets().into_iter().find(|(_, w)| self.bounds(w).is_some_and(|b| b.contains_point(&p))).map(|(n, _)| n)
    }

    /// Where a block dropped at x, y goes: its part, its index among the part's other blocks; None off the bar.
    fn target(&self, x: f64, y: f64, name: &str) -> Option<(usize, usize)> {
        if y > crate::popup::bar() as f64 + 12.0 {
            return None;
        }
        let parts = self.bar.parts();
        let part = parts.iter().position(|p| self.bounds(p).is_some_and(|b| (b.x() as f64..(b.x() + b.width()) as f64).contains(&x)));
        let part = part.unwrap_or(if x < self.bar.strip.width() as f64 / 2.0 { 0 } else { 2 });
        let widgets = self.bar.widgets();
        let layout = self.layout.borrow();
        let before = layout[part].iter().filter(|n| *n != name).take_while(|n| {
            let w = widgets.iter().find(|(m, _)| m == *n).and_then(|(_, w)| self.bounds(w));
            w.is_some_and(|b| ((b.x() + b.width() / 2.0) as f64) < x)
        });
        Some((part, before.count()))
    }

    /// The pointer moved with a press: past the slop a drag, the place it would land marked, a block leaving the
    /// bar faded.
    fn over(&self, x: f64, y: f64, dist: f64) {
        let name = {
            let mut d = self.drag.borrow_mut();
            let Some(d) = d.as_mut() else { return };
            let Some(name) = d.name.clone() else { return };
            if !d.moving && dist < SLOP {
                return;
            }
            d.moving = true;
            name
        };
        let to = self.target(x, y, &name);
        let dragged = self.bar.widgets().into_iter().find(|(n, _)| *n == name).map(|(_, w)| w);
        if let Some(w) = &dragged {
            w.set_opacity(if to.is_some() { 0.4 } else { 0.15 });
        }
        match to {
            Some((p, i)) => self.mark(p, i, &name),
            None => self.unmark(),
        }
    }

    /// The marker put where a block would land: before the part's i-th other block, else at its end.
    fn mark(&self, p: usize, i: usize, name: &str) {
        self.unmark();
        let part = &self.bar.parts()[p];
        let widgets = self.bar.widgets();
        let next = self.layout.borrow()[p].iter().filter(|n| *n != name).nth(i).cloned();
        let next = next.and_then(|n| widgets.into_iter().find(|(m, _)| *m == n).map(|(_, w)| w));
        // the right's end, the black at the screen's edge, stays last
        let next = next.or_else(|| if p == 2 { part.last_child() } else { None });
        match next {
            Some(n) => self.marker.insert_before(part, Some(&n)),
            None => part.append(&self.marker),
        }
    }

    fn unmark(&self) {
        if let Some(p) = self.marker.parent().and_downcast::<gtk4::Box>() {
            p.remove(&self.marker);
        }
    }

    /// The press let go: a drag lands (or a block dragged off the bar leaves it); a click picks a bar's block, or
    /// puts a gallery's after the picked one.
    fn dropped(&self, x: f64, y: f64) {
        let Some(d) = self.drag.take() else { return };
        self.unmark();
        let Some(name) = d.name else { return };
        let layout = self.layout.borrow().clone();
        let new = if d.moving {
            match self.target(x, y, &name) {
                Some(to) => moved(&layout, &name, Some(to)),
                None if d.from_bar => moved(&layout, &name, None),
                None => return self.apply(layout),
            }
        } else if d.from_bar {
            let now = if self.picked.borrow().as_deref() == Some(&name) { None } else { Some(name) };
            *self.picked.borrow_mut() = now;
            return self.redraw();
        } else {
            let picked = self.picked.borrow().clone();
            let at = picked.and_then(|p| place(&layout, &p)).map_or((2, layout[2].len()), |(p, i)| (p, i + 1));
            moved(&layout, &name, Some(at))
        };
        self.apply(new);
    }

    /// The keys: Tab picks the next block, Shift+arrows move it, Delete takes it off, Enter Done, Escape Cancel.
    /// Left to an entry while one has the focus (the new panel's name).
    fn key(&self, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
        let focus = gtk4::prelude::GtkWindowExt::focus(&self.host.win);
        if focus.is_some_and(|f: gtk4::Widget| f.is::<gtk4::Text>() || f.is::<gtk4::Entry>()) {
            return glib::Propagation::Proceed;
        }
        let layout = self.layout.borrow().clone();
        let picked = self.picked.borrow().clone();
        let all: Vec<String> = layout.iter().flatten().cloned().collect();
        match key {
            gdk::Key::Escape => finish(false),
            gdk::Key::Return | gdk::Key::KP_Enter => finish(true),
            gdk::Key::Tab | gdk::Key::ISO_Left_Tab if !all.is_empty() => {
                let back = key == gdk::Key::ISO_Left_Tab || mods.contains(gdk::ModifierType::SHIFT_MASK);
                let at = picked.and_then(|p| all.iter().position(|n| *n == p));
                let n = all.len();
                let next = match (at, back) {
                    (None, false) => 0,
                    (None, true) => n - 1,
                    (Some(i), false) => (i + 1) % n,
                    (Some(i), true) => (i + n - 1) % n,
                };
                *self.picked.borrow_mut() = Some(all[next].clone());
                self.redraw();
            }
            gdk::Key::Left | gdk::Key::Right if mods.contains(gdk::ModifierType::SHIFT_MASK) => {
                let Some(p) = picked else { return glib::Propagation::Stop };
                let to = step(&layout, &p, key == gdk::Key::Right);
                self.apply(moved(&layout, &p, to));
            }
            gdk::Key::Delete | gdk::Key::BackSpace => {
                let Some(p) = picked else { return glib::Propagation::Stop };
                *self.picked.borrow_mut() = None;
                self.apply(moved(&layout, &p, None));
            }
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    /// The bar laid out so, at once; the gallery and the marks drawn again.
    fn apply(&self, layout: [Vec<String>; 3]) {
        self.bar.relayout(&layout);
        *self.layout.borrow_mut() = layout;
        self.redraw();
    }

    /// The gallery: every block not in the bar, by kind, and a new panel's name; the picked block marked.
    fn redraw(&self) {
        let layout = self.layout.borrow().clone();
        let picked = self.picked.borrow().clone();
        for (n, w) in self.bar.widgets() {
            w.set_opacity(1.0);
            if picked.as_deref() == Some(n.as_str()) {
                w.add_css_class("edit-picked");
            } else {
                w.remove_css_class("edit-picked");
            }
        }
        crate::style::clear(&self.shelf);
        let placed = |id: &str| layout.iter().flatten().any(|p| p == id || named(p).0 == named(id).0);
        for (title, ids) in every() {
            let ids: Vec<String> = ids.into_iter().filter(|id| !placed(id)).collect();
            if ids.is_empty() {
                continue;
            }
            self.shelf.append(&label(t(title), "dim"));
            let flow = crate::ui::chip_flow();
            for id in ids {
                let (name, icon) = named(&id);
                let chip = gtk4::Box::new(Orientation::Horizontal, 6);
                chip.add_css_class("chip");
                chip.add_css_class("gallery-item");
                chip.set_widget_name(&id);
                chip.set_cursor_from_name(Some("grab"));
                chip.append(&gtk4::Image::from_icon_name(&icon));
                chip.append(&gtk4::Label::new(Some(&name)));
                crate::ui::flow_in(&flow, &chip);
            }
            self.shelf.append(&flow);
        }
        // a new panel: its name, its id made of it, put at the right's end
        let new = gtk4::Entry::new();
        new.set_placeholder_text(Some(t("New panel's name…")));
        new.connect_activate(|e| {
            let name = e.text().trim().to_string();
            let id: String = name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
            let id = id.trim_matches('-').to_string();
            let Some(me) = current().filter(|_| !id.is_empty() && id != "control" && id != "calendar") else { return };
            if let Err(err) = crate::settings::write(&format!("panels.{id}"), "name", Some(&json!(name)), false) {
                return eprintln!("ostrov: bar: {err}");
            }
            let layout = me.layout.borrow().clone();
            let end = layout[2].len();
            me.apply(moved(&layout, &format!("panel.{id}"), Some((2, end))));
        });
        self.shelf.append(&new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(a: &[&str], b: &[&str], c: &[&str]) -> [Vec<String>; 3] {
        [a, b, c].map(|v| v.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn blocks_moved() {
        let now = l(&["workspaces", "window"], &["panel.calendar"], &["tray", "panel.control"]);
        assert_eq!(moved(&now, "tray", Some((0, 1))), l(&["workspaces", "tray", "window"], &["panel.calendar"], &["panel.control"]));
        assert_eq!(moved(&now, "window", None), l(&["workspaces"], &["panel.calendar"], &["tray", "panel.control"]));
        assert_eq!(moved(&now, "layout", Some((1, 9))), l(&["workspaces", "window"], &["panel.calendar", "layout"], &["tray", "panel.control"]));
        // along the bar a step at a time, across the parts' ends
        assert_eq!(step(&now, "window", false), Some((0, 0)));
        assert_eq!(step(&now, "window", true), Some((1, 0)));
        assert_eq!(step(&now, "tray", false), Some((1, 1)));
        assert_eq!(step(&now, "panel.control", true), Some((2, 1)));
        assert_eq!(step(&now, "workspaces", false), Some((0, 0)));
        assert_eq!(place(&now, "tray"), Some((2, 0)));
    }
}
