//! The bar's window and the popups grown out of it, one surface. The window is the screen's height, the bar its strip
//! at the top, the windows kept clear of that strip alone; it takes input over the strip and an open popup's
//! column only, so everything else on the screen goes on getting its clicks. A popup is laid over the window
//! under the bar: while it is open the block it grows out of is a tab, and the popup's top edge runs from its
//! corners to the tab and is open under it, so tab and popup are one shape, one ground, one blur. It unrolls
//! down out of the tab as it opens and rolls back up as it closes, GNOME's way and short. A click anywhere else
//! (Hyprland's focus grab tells), its tab again, or Escape closes it.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

/// The bar's height, the strip the windows keep clear of.
pub const BAR: i32 = 25;

/// The bar's window: its mode (i3's bar mode toggle: docked, or hidden and shown over the windows while Super
/// is held), the launcher in it, the popup open over it.
pub struct Host {
    pub win: gtk4::ApplicationWindow,
    layer: gtk4::Overlay,
    pub docked: Cell<bool>,
    pub peeking: Cell<bool>,
    pub launching: Cell<bool>,
    open: RefCell<Option<Weak<Popup>>>,
    grab: RefCell<Option<crate::wm::Grab>>,
}

impl Host {
    /// The bar's window over the whole screen, strip (the bar) at its top.
    pub fn new(app: &gtk4::Application, strip: &impl IsA<gtk4::Widget>) -> Rc<Host> {
        let win = gtk4::ApplicationWindow::new(app);
        win.init_layer_shell();
        win.set_namespace(Some("ostrov"));
        // the top and both sides, as tall as the screen: anchored to all four edges its strip would be ignored
        for e in [Edge::Top, Edge::Left, Edge::Right] {
            win.set_anchor(e, true);
        }
        let height = gtk4::gdk::Display::default()
            .and_then(|d| d.monitors().item(0))
            .and_downcast::<gtk4::gdk::Monitor>()
            .map_or(1080, |m| m.geometry().height());
        let col = gtk4::Box::new(Orientation::Vertical, 0);
        strip.set_size_request(-1, BAR);
        col.append(strip);
        let layer = gtk4::Overlay::new();
        layer.set_child(Some(&col));
        layer.set_size_request(-1, height);
        win.set_child(Some(&layer));

        let host = Rc::new(Host {
            win: win.clone(),
            layer,
            docked: Cell::new(true),
            peeking: Cell::new(false),
            launching: Cell::new(false),
            open: RefCell::default(),
            grab: RefCell::default(),
        });

        // Escape, a click under the bar beside or under the popup: closed
        let keys = gtk4::EventControllerKey::new();
        let h = Rc::downgrade(&host);
        keys.connect_key_pressed(move |_, k, _, _| {
            if let Some(p) = h.upgrade().and_then(|h| h.popup()).filter(|_| k == gtk4::gdk::Key::Escape) {
                p.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        win.add_controller(keys);
        let click = gtk4::GestureClick::new();
        let h = Rc::downgrade(&host);
        click.connect_released(move |_, _, _, y| {
            if let Some(p) = h.upgrade().and_then(|h| h.popup()).filter(|p| y >= BAR as f64 && !p.contains(y)) {
                p.close();
            }
        });
        win.add_controller(click);
        // without a grab (sway) a click into a window, caught by the keyboard it takes
        let h = Rc::downgrade(&host);
        win.connect_is_active_notify(move |w| {
            let Some(h) = h.upgrade() else { return };
            if !w.is_active() && h.grab.borrow().is_none() {
                if let Some(p) = h.popup() {
                    p.close();
                }
            }
        });
        host.apply();
        win.present();
        host
    }

    /// The popup open, if one is.
    pub fn popup(&self) -> Option<Rc<Popup>> {
        self.open.borrow().as_ref().and_then(Weak::upgrade).filter(|p| p.is_open())
    }

    /// The window set for its mode and what is open in it: its layer and strip, the keyboard (the launcher's
    /// alone, a popup's on demand for a passphrase and Escape), shown or not, input where it is drawn.
    pub fn apply(&self) {
        let popup = self.popup();
        if self.docked.get() {
            self.win.set_layer(Layer::Top);
            self.win.set_exclusive_zone(BAR);
        } else {
            self.win.set_layer(Layer::Overlay);
            self.win.set_exclusive_zone(0);
        }
        self.win.set_keyboard_mode(if self.launching.get() {
            KeyboardMode::Exclusive
        } else if popup.is_some() {
            KeyboardMode::OnDemand
        } else {
            KeyboardMode::None
        });
        self.win.set_visible(self.docked.get() || self.peeking.get() || self.launching.get() || popup.is_some());
        self.region(popup.as_ref().map(|p| p.column()));
    }

    /// Input over the strip and a popup's column (x, width) down to the screen's bottom.
    fn region(&self, column: Option<(i32, i32)>) {
        let Some(surface) = self.win.surface() else { return };
        let r = gtk4::cairo::Region::create_rectangle(&gtk4::cairo::RectangleInt::new(0, 0, self.win.width().max(1), BAR));
        if let Some((x, w)) = column {
            let _ = r.union_rectangle(&gtk4::cairo::RectangleInt::new(x, BAR, w, self.win.height() - BAR));
        }
        surface.set_input_region(Some(&r));
    }

    /// The popup now open: the input over it once it is laid out, the focus grab on the window.
    fn opened(self: &Rc<Self>, p: &Rc<Popup>) {
        *self.open.borrow_mut() = Some(Rc::downgrade(p));
        self.apply();
        // its column known from its first frame laid out
        let h = Rc::downgrade(self);
        let p2 = Rc::downgrade(p);
        p.reveal.add_tick_callback(move |_, _| {
            let (Some(h), Some(p)) = (h.upgrade(), p2.upgrade()) else { return glib::ControlFlow::Break };
            let (x, w) = p.column();
            if w == 0 {
                return glib::ControlFlow::Continue;
            }
            if h.popup().is_some_and(|o| Rc::ptr_eq(&o, &p)) {
                h.region(Some((x, w)));
            }
            glib::ControlFlow::Break
        });
        let h = Rc::downgrade(self);
        *self.grab.borrow_mut() = crate::wm::grab(&[self.win.clone().upcast()], move || {
            if let Some(p) = h.upgrade().and_then(|h| h.popup()) {
                p.close();
            }
        });
    }

    /// No popup open any more.
    fn closed(&self) {
        self.grab.take();
        self.apply();
        crate::wm::nudge();
    }
}

/// Where the popup hangs: its right edge under its tab's (the bar's right end, the tray), or in the middle.
#[derive(Clone, Copy, PartialEq)]
pub enum Side {
    Right,
    Center,
}

pub struct Popup {
    host: Weak<Host>,
    reveal: gtk4::Revealer,
    shape: gtk4::Box,
    /// the bar's block it grows out of; the tray's menu changes it to the icon clicked
    tab: RefCell<gtk4::Widget>,
    side: Side,
    gap: gtk4::Box,
    on_open: RefCell<Vec<Box<dyn Fn()>>>,
    closed: Cell<Option<std::time::Instant>>,
}

impl Popup {
    /// A popup of width (-1: its own) hanging from tab, its body (a box of class surface) inside.
    pub fn new(host: &Rc<Host>, tab: &impl IsA<gtk4::Widget>, side: Side, width: i32, body: &gtk4::Box) -> Rc<Popup> {
        let reveal = gtk4::Revealer::new();
        reveal.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        reveal.set_transition_duration(120);
        reveal.set_valign(Align::Start);
        reveal.set_halign(if side == Side::Right { Align::End } else { Align::Center });
        reveal.set_margin_top(BAR);
        reveal.set_size_request(width, -1);

        // the shape: a top edge from the corners to the tab, open under it, then the body without a top edge
        let shape = gtk4::Box::new(Orientation::Vertical, 0);
        let top = gtk4::Box::new(Orientation::Horizontal, 0);
        let left = gtk4::Box::new(Orientation::Horizontal, 0);
        left.add_css_class("edge");
        left.set_hexpand(true);
        let gap = gtk4::Box::new(Orientation::Horizontal, 0);
        top.append(&left);
        top.append(&gap);
        if side == Side::Right {
            gap.add_css_class("gap");
        } else {
            gap.add_css_class("gap-mid");
            let right = gtk4::Box::new(Orientation::Horizontal, 0);
            right.add_css_class("edge-right");
            right.set_hexpand(true);
            top.append(&right);
        }
        shape.append(&top);
        body.add_css_class("attached");
        shape.append(body);
        reveal.set_child(Some(&shape));
        host.layer.add_overlay(&reveal);

        let popup = Rc::new(Popup {
            host: Rc::downgrade(host),
            reveal: reveal.clone(),
            shape,
            tab: RefCell::new(tab.clone().upcast()),
            side,
            gap,
            on_open: RefCell::default(),
            closed: Cell::default(),
        });
        // rolled up: the tab a block again, the input back to the strip
        let p = Rc::downgrade(&popup);
        reveal.connect_child_revealed_notify(move |r| {
            let Some(p) = p.upgrade().filter(|_| !r.is_child_revealed() && !r.reveals_child()) else { return };
            p.tab.borrow().remove_css_class("tab");
            if let Some(h) = p.host.upgrade().filter(|h| h.popup().is_none()) {
                h.closed();
            }
        });
        popup
    }

    pub fn is_open(&self) -> bool {
        self.reveal.reveals_child()
    }

    /// Its column in the window: x and width.
    fn column(&self) -> (i32, i32) {
        let Some(b) = self.host.upgrade().and_then(|h| self.reveal.compute_bounds(&h.win)) else { return (0, 0) };
        (b.x() as i32, b.width() as i32)
    }

    /// y in the window within the popup's shape.
    fn contains(&self, y: f64) -> bool {
        y < BAR as f64 + self.shape.height() as f64
    }

    /// f on every opening (the panel folds its menus, the calendar goes back to this month).
    pub fn on_open(&self, f: impl Fn() + 'static) {
        self.on_open.borrow_mut().push(Box::new(f));
    }

    /// Opened, any other popup closed.
    pub fn open(self: &Rc<Self>) {
        let Some(host) = self.host.upgrade() else { return };
        if let Some(other) = host.popup().filter(|o| !Rc::ptr_eq(o, self)) {
            other.close();
        }
        for f in self.on_open.borrow().iter() {
            f();
        }
        self.place();
        self.tab.borrow().add_css_class("tab");
        self.reveal.set_reveal_child(true);
        host.opened(self);
    }

    /// Hung from another block (the tray's icons share one popup).
    pub fn set_tab(&self, tab: &impl IsA<gtk4::Widget>) {
        let open = self.is_open();
        if open {
            self.tab.borrow().remove_css_class("tab");
        }
        *self.tab.borrow_mut() = tab.clone().upcast();
        if open {
            self.place();
            self.tab.borrow().add_css_class("tab");
        }
    }

    pub fn tab(&self) -> gtk4::Widget {
        self.tab.borrow().clone()
    }

    /// The gap as wide as the tab's border box (width() is its content alone); at the right, the popup's right
    /// edge under the tab's.
    fn place(&self) {
        let Some(host) = self.host.upgrade() else { return };
        let tab = self.tab.borrow();
        let Some(b) = tab.compute_bounds(&host.win) else { return };
        let inner = if self.side == Side::Right { 1 } else { 2 };
        let tw = b.width().round() as i32;
        if tw > inner {
            self.gap.set_size_request(tw - inner, -1);
        }
        if self.side == Side::Right {
            self.reveal.set_margin_end(host.win.width() - (b.x() + b.width()).round() as i32);
        }
    }

    pub fn close(&self) {
        if self.is_open() {
            self.closed.set(Some(std::time::Instant::now()));
        }
        self.reveal.set_reveal_child(false);
    }

    /// Open or close; closed a moment ago, it stays closed: the click on its tab that toggles it is the one
    /// that cleared the grab and closed it just before.
    pub fn toggle(self: &Rc<Self>) {
        if self.is_open() {
            self.close()
        } else if !self.closed.get().is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(300)) {
            self.open()
        }
    }
}
