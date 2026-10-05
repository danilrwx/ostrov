//! The bar, made of blocks: each a module of its own (bar/*.rs) built by name, the names in three lists, left,
//! middle and right. A block is a widget in the bar, and may have a popup grown out of it (the clock's calendar,
//! the status's quick settings, the tray's menus) and commands of its own (ostrov BLOCK ARGS). The bar's black is
//! laid by its parts (style.rs): the left and the right are black wholes, a block with a popup sits in a slot
//! whose black goes while it is hovered or a tab.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use gtk4::prelude::*;
use gtk4::glib;

use crate::hub::Hub;
use crate::popup::{Host, Popup, Side};

pub mod edit;
mod panel;
mod privacy;
mod tray;
mod widget;
mod window;
mod workspaces;

pub use window::app_icon;

thread_local! {
    /// every bar's blocks, for the compositor's events: one connection to it, started with the first bar
    static BARS: RefCell<Option<Vec<Weak<Ctx>>>> = const { RefCell::new(None) };
}

/// What a block is built with.
pub struct Ctx {
    pub host: Rc<Host>,
    pub hub: Rc<Hub>,
    /// the compositor's events, each to every block that asked (wm.rs)
    wm: RefCell<Vec<Box<dyn Fn(&crate::wm::Event)>>>,
}

impl Ctx {
    /// f on every event of the compositor's.
    pub fn on_wm(&self, f: impl Fn(&crate::wm::Event) + 'static) {
        self.wm.borrow_mut().push(Box::new(f));
    }
}

/// A command of a block's: what it prints, or what went wrong.
pub type Command = Box<dyn Fn(&[&str]) -> Result<String, String>>;

/// A block of the bar.
pub struct Block {
    pub widget: gtk4::Widget,
    pub popup: Option<Rc<Popup>>,
    pub command: Option<Command>,
    /// its command's forms (forms.rs), for its usage and the shells' completion
    pub forms: &'static [&'static str],
}

impl Block {
    pub fn new(widget: &impl IsA<gtk4::Widget>) -> Block {
        Block { widget: widget.clone().upcast(), popup: None, command: None, forms: &[] }
    }
}

/// The blocks there are, by name; a panel's, panel.ID, unrolling on the side of the bar it is in; any widget's,
/// widget.ID, its badge in the bar. The names from
/// before there were panels still work: status the control centre's, clock the calendar's.
fn block(name: &str, cx: &Rc<Ctx>, side: Side) -> Option<Block> {
    // a panel's widget on its own, its badge the block
    if let Some(id) = name.strip_prefix("widget.") {
        return widget::build(cx, id, side);
    }
    Some(match name {
        "workspaces" => workspaces::build(cx),
        "window" => window::build(cx),
        // the keyboard layout's widget, by its name from before it was one
        "layout" => return widget::build(cx, "keymap", side),
        "privacy" => privacy::build(cx, side),
        "tray" => tray::build(cx),
        "status" => panel::build(cx, "control", side),
        "clock" => panel::build(cx, "calendar", side),
        n if n.starts_with("panel.") => panel::build(cx, &n["panel.".len()..], side),
        _ => {
            eprintln!("ostrov: no block {name}");
            return None;
        }
    })
}

/// The parts' sides, the popups of their blocks hanging so.
const SIDES: [Side; 3] = [Side::Left, Side::Center, Side::Right];

/// The left's first block kept off the screen's edge.
fn edge_margin(left: &gtk4::Box) {
    if let Some(first) = left.first_child() {
        first.set_margin_start(12);
    }
}

/// A block's pill: its content in a row, the ground lit under the pointer or as a tab.
pub fn pill() -> gtk4::Box {
    let gap = || crate::config::load().appearance.bar_spacing.min(32) as i32;
    let b = gtk4::Box::new(gtk4::Orientation::Horizontal, gap());
    b.add_css_class("pill");
    // its icons' gap as the config says it now
    let w = b.downgrade();
    crate::style::on_config(move || {
        if let Some(b) = w.upgrade() {
            b.set_spacing(gap());
        }
    });
    b
}

/// A block's place in the bar: the black around it, gone while it is hovered or a tab (see the CSS).
pub fn slot(w: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let s = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    s.add_css_class("slot");
    s.set_overflow(gtk4::Overflow::Hidden);
    s.append(w);
    s
}

pub struct Bar {
    pub strip: gtk4::CenterBox,
    cx: Rc<Ctx>,
    left: gtk4::Box,
    center: gtk4::Box,
    right: gtk4::Box,
    blocks: RefCell<Vec<(String, Block)>>,
    /// the right's last piece, the black at the screen's edge: the right's blocks go before it
    end: gtk4::Box,
}

impl Bar {
    /// The bar of these blocks, left, middle and right.
    pub fn build(host: &Rc<Host>, hub: &Rc<Hub>, layout: [&[&str]; 3]) -> Rc<Bar> {
        let cx = Rc::new(Ctx {
            host: host.clone(),
            hub: hub.clone(),
            wm: RefCell::default(),
        });
        let strip = gtk4::CenterBox::new();
        let left = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        left.add_css_class("bar-bg");
        left.set_hexpand(true);
        let center = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let right = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        right.set_hexpand(true);
        // the right's black up to its blocks, and past them at the screen's edge
        let fill = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        fill.add_css_class("bar-bg");
        fill.set_hexpand(true);
        right.append(&fill);

        let mut blocks = Vec::new();
        for ((names, into), side) in layout.iter().zip([&left, &center, &right]).zip(SIDES) {
            for name in names.iter() {
                if let Some(b) = block(name, &cx, side) {
                    into.append(&b.widget);
                    blocks.push((name.to_string(), b));
                }
            }
        }
        let end = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        end.add_css_class("bar-bg");
        end.set_size_request(6, -1);
        right.append(&end);
        edge_margin(&left);
        strip.set_start_widget(Some(&left));
        strip.set_center_widget(Some(&center));
        strip.set_end_widget(Some(&right));

        let bar = Rc::new(Bar { strip, cx: cx.clone(), left, center, right, blocks: RefCell::new(blocks), end });
        let first = BARS.with(|b| {
            let mut b = b.borrow_mut();
            let first = b.is_none();
            let all = b.get_or_insert_default();
            all.retain(|w| w.strong_count() > 0);
            all.push(Rc::downgrade(&cx));
            first
        });
        if first {
            listen();
        }

        // a click elsewhere in the bar than a block with a popup closes the one open
        let click = gtk4::GestureClick::new();
        let b = Rc::downgrade(&bar);
        click.connect_released(move |g, _, x, y| {
            let (Some(b), Some(w)) = (b.upgrade(), g.widget().filter(|_| y < crate::popup::bar() as f64)) else { return };
            let hit = w.pick(x, y, gtk4::PickFlags::DEFAULT);
            let on_popup_block = b.blocks.borrow().iter().any(|(_, k)| k.popup.is_some() && hit.as_ref().is_some_and(|h| h.is_ancestor(&k.widget) || *h == k.widget));
            if !on_popup_block
                && let Some(p) = b.cx.host.popup() {
                    p.close();
                }
        });
        cx.host.win.add_controller(click);
        // a right click on the bar beside its blocks: its editor
        let right = gtk4::GestureClick::new();
        right.set_button(3);
        let b = Rc::downgrade(&bar);
        right.connect_released(move |g, _, x, y| {
            let (Some(b), Some(w)) = (b.upgrade(), g.widget().filter(|_| y < crate::popup::bar() as f64)) else { return };
            let hit = w.pick(x, y, gtk4::PickFlags::DEFAULT);
            let on_block = b.blocks.borrow().iter().any(|(_, k)| hit.as_ref().is_some_and(|h| h.is_ancestor(&k.widget) || *h == k.widget));
            if !on_block
                && edit::is_open().is_none()
                && let Err(e) = edit::start()
            {
                eprintln!("ostrov: bar: {e}");
            }
        });
        cx.host.win.add_controller(right);
        bar
    }

    /// Its blocks laid out anew, left, middle and right, as the bar's editor moves them (bar/edit.rs): the ones it
    /// has moved where they go (their popups hung on their new side), new ones built, the ones left out gone. A
    /// block is moved, not built again: a panel's face and the tray stay what they are.
    pub fn relayout(&self, layout: &[Vec<String>; 3]) {
        let mut old: Vec<(String, Block)> = self.blocks.take();
        for (_, b) in &old {
            if let Some(p) = b.widget.parent().and_downcast::<gtk4::Box>() {
                p.remove(&b.widget);
            }
        }
        let mut now = Vec::new();
        for ((names, into), side) in layout.iter().zip([&self.left, &self.center, &self.right]).zip(SIDES) {
            for name in names {
                let b = match old.iter().position(|(n, _)| n == name) {
                    Some(i) => Some(old.remove(i).1),
                    None => block(name, &self.cx, side),
                };
                let Some(b) = b else { continue };
                if let Some(p) = &b.popup {
                    p.set_side(side);
                }
                b.widget.set_margin_start(0);
                if into == &self.right {
                    b.widget.insert_before(&self.right, Some(&self.end));
                } else {
                    into.append(&b.widget);
                }
                now.push((name.clone(), b));
            }
        }
        // a block left out: its popup closed, gone with it
        for (_, b) in old {
            if let Some(p) = &b.popup {
                p.close();
            }
        }
        edge_margin(&self.left);
        *self.blocks.borrow_mut() = now;
    }

    /// The parts' boxes, left, middle and right, and each block's name and widget: what the editor lays out.
    pub fn parts(&self) -> [gtk4::Box; 3] {
        [self.left.clone(), self.center.clone(), self.right.clone()]
    }

    pub fn widgets(&self) -> Vec<(String, gtk4::Widget)> {
        self.blocks.borrow().iter().map(|(n, b)| (n.clone(), b.widget.clone())).collect()
    }

    pub fn host(&self) -> Rc<Host> {
        self.cx.host.clone()
    }

    /// Its blocks told their monitor changed (the workspaces': that monitor's).
    pub fn moved(&self) {
        for f in self.cx.wm.borrow().iter() {
            f(&crate::wm::Event::Workspaces);
        }
    }

    /// The name of the block whose popup is open.
    pub fn open(&self) -> Option<String> {
        self.blocks.borrow().iter().find(|(_, b)| b.popup.as_ref().is_some_and(|p| p.is_open())).map(|(n, _)| n.clone())
    }

    /// A block's command (ostrov BLOCK ARGS); None when there is no such block or it has none.
    pub fn command(&self, name: &str, args: &[&str]) -> Option<Result<String, String>> {
        self.blocks.borrow().iter().find(|(n, _)| n == name).and_then(|(_, b)| b.command.as_ref()).map(|c| c(args))
    }

    /// Whether it has a block of that name with a command.
    pub fn has_command(&self, name: &str) -> bool {
        self.blocks.borrow().iter().any(|(n, b)| n == name && b.command.is_some())
    }

    /// The blocks with commands, and their forms.
    pub fn forms(&self) -> Vec<(String, &'static [&'static str])> {
        self.blocks.borrow().iter().filter(|(_, b)| !b.forms.is_empty()).map(|(n, b)| (n.clone(), b.forms)).collect()
    }

    /// The span between the left's blocks and the right's, the launcher's place: its start and end from the
    /// bar's left and right edges.
    pub fn middle(&self) -> (i32, i32) {
        let x = |w: &gtk4::Widget| w.compute_bounds(&self.strip).map_or((0, 0), |b| (b.x() as i32, (b.x() + b.width()) as i32));
        let start = self.left.last_child().map_or(0, |w| x(&w).1);
        // the right's first block, past its fill
        let end = self.right.first_child().and_then(|f| f.next_sibling()).map_or(self.strip.width(), |w| x(&w).0);
        (start, self.strip.width() - end)
    }

    /// The middle's blocks hidden (their black kept) while the launcher takes their place.
    pub fn hide_middle(&self, hide: bool) {
        let mut c = self.center.first_child();
        while let Some(w) = c {
            if let Some(inner) = w.first_child() {
                inner.set_opacity(if hide { 0.0 } else { 1.0 });
            }
            w.set_can_target(!hide);
            c = w.next_sibling();
        }
    }
}

/// The compositor's events to every bar's blocks, and said once (events.rs); a Super combination done ends a peek.
fn listen() {
    let (tx, rx) = async_channel::unbounded();
    std::thread::spawn(move || crate::wm::events(tx));
    glib::spawn_future_local(async move {
        while let Ok(e) = rx.recv().await {
            let bars: Vec<Rc<Ctx>> = BARS.with(|b| b.borrow().iter().flatten().filter_map(Weak::upgrade).collect());
            for cx in &bars {
                for f in cx.wm.borrow().iter() {
                    f(&e);
                }
                if matches!(e, crate::wm::Event::Done) && cx.host.peeking.get() {
                    cx.host.peeking.set(false);
                    cx.host.apply();
                }
            }
            match &e {
                crate::wm::Event::Window(class, title) => {
                    crate::events::emit("window", serde_json::json!({"class": class, "title": title}))
                }
                crate::wm::Event::Workspaces => crate::events::emit("workspace", serde_json::json!({})),
                crate::wm::Event::Screencast(on, window) => {
                    crate::events::emit("screencast", serde_json::json!({"on": on, "window": window}))
                }
                crate::wm::Event::Done => {}
            }
        }
    });
}
