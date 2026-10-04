//! The bar, made of blocks: each a module of its own (bar/*.rs) built by name, the names in three lists, left,
//! middle and right. A block is a widget in the bar, and may have a popup grown out of it (the clock's calendar,
//! the status's quick settings, the tray's menus) and commands of its own (ostrov BLOCK ARGS). The bar's black is
//! laid by its parts (style.rs): the left and the right are black wholes, a block with a popup sits in a slot
//! whose black goes while it is hovered or a tab.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::glib;

use crate::hub::Hub;
use crate::popup::{Host, Popup, Side, BAR};

mod layout;
mod panel;
mod privacy;
mod tray;
mod window;
mod workspaces;

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

/// The blocks there are, by name; a panel's, panel.ID, unrolling on the side of the bar it is in. The names from
/// before there were panels still work: status the control centre's, clock the calendar's.
fn block(name: &str, cx: &Rc<Ctx>, side: Side) -> Option<Block> {
    Some(match name {
        "workspaces" => workspaces::build(cx),
        "window" => window::build(cx),
        "layout" => layout::build(cx),
        "privacy" => privacy::build(cx),
        "tray" => tray::build(cx),
        "status" => panel::build(cx, "control", side),
        "clock" => panel::build(cx, "calendar", side),
        n if n.starts_with("panel.") => panel::build(cx, &n["panel.".len()..], side),
        "record" => crate::record::block(),
        _ => {
            eprintln!("ostrov: no block {name}");
            return None;
        }
    })
}

/// A block's pill: its content in a row, the ground lit under the pointer or as a tab.
pub fn pill() -> gtk4::Box {
    let b = gtk4::Box::new(gtk4::Orientation::Horizontal, 7);
    b.add_css_class("pill");
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
    left: gtk4::Box,
    center: gtk4::Box,
    right: gtk4::Box,
    blocks: Vec<(String, Block)>,
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
        for ((names, into), side) in layout.iter().zip([&left, &center, &right]).zip([Side::Left, Side::Center, Side::Right]) {
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
        if let Some(first) = left.first_child() {
            first.set_margin_start(12);
        }
        strip.set_start_widget(Some(&left));
        strip.set_center_widget(Some(&center));
        strip.set_end_widget(Some(&right));

        let bar = Rc::new(Bar { strip, left, center, right, blocks });

        // the compositor's events to the blocks; a Super combination done ends a peek
        let (tx, rx) = async_channel::unbounded();
        std::thread::spawn(move || crate::wm::events(tx));
        let (cx2, host) = (cx.clone(), host.clone());
        glib::spawn_future_local(async move {
            while let Ok(e) = rx.recv().await {
                for f in cx2.wm.borrow().iter() {
                    f(&e);
                }
                if matches!(e, crate::wm::Event::Done) && host.peeking.get() {
                    host.peeking.set(false);
                    host.apply();
                }
            }
        });

        // a click elsewhere in the bar than a block with a popup closes the one open
        let click = gtk4::GestureClick::new();
        let b = Rc::downgrade(&bar);
        let host = cx.host.clone();
        click.connect_released(move |g, _, x, y| {
            let (Some(b), Some(w)) = (b.upgrade(), g.widget().filter(|_| y < BAR as f64)) else { return };
            let hit = w.pick(x, y, gtk4::PickFlags::DEFAULT);
            let on_popup_block = b.blocks.iter().any(|(_, k)| k.popup.is_some() && hit.as_ref().is_some_and(|h| h.is_ancestor(&k.widget) || *h == k.widget));
            if !on_popup_block {
                if let Some(p) = host.popup() {
                    p.close();
                }
            }
        });
        cx.host.win.add_controller(click);
        bar
    }

    /// The name of the block whose popup is open.
    pub fn open(&self) -> Option<&str> {
        self.blocks.iter().find(|(_, b)| b.popup.as_ref().is_some_and(|p| p.is_open())).map(|(n, _)| n.as_str())
    }

    /// A block's command (ostrov BLOCK ARGS); None when there is no such block or it has none.
    pub fn command(&self, name: &str, args: &[&str]) -> Option<Result<String, String>> {
        self.blocks.iter().find(|(n, _)| n == name).and_then(|(_, b)| b.command.as_ref()).map(|c| c(args))
    }

    /// The blocks with commands, and their forms.
    pub fn forms(&self) -> Vec<(String, &'static [&'static str])> {
        self.blocks.iter().filter(|(_, b)| !b.forms.is_empty()).map(|(n, b)| (n.clone(), b.forms)).collect()
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
