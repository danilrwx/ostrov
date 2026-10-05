//! The bar's window and the popups grown out of it, one surface. The window is the screen's height, the bar its strip
//! at the top, the windows kept clear of that strip alone; it takes input over the strip and an open popup's
//! column only, so everything else on the screen goes on getting its clicks. A popup is laid over the window
//! under the bar: while it is open the block it grows out of is a tab, and the popup's top edge runs from its
//! corners to the tab and is open under it, so tab and popup are one shape, one ground, one blur. It unrolls
//! down out of the tab as it opens and rolls back up as it closes, GNOME's way and short. A click anywhere else
//! (Hyprland's focus grab tells), its tab again, or Escape closes it.
//!
//! A bar on every monitor (bars.rs), a window each. A panel's popup is one, its block in every bar a tab of it: a
//! click opens it under the tab clicked, moved there from another bar's; a command (ostrov panel) under the focused
//! monitor's.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

thread_local! {
    static HEIGHT: Cell<i32> = Cell::new(height());
    /// the bars' windows, the first the launcher's
    static HOSTS: RefCell<Vec<Weak<Host>>> = RefCell::default();
}

/// The bar's height in pixels as [appearance]'s bar_height says it now, kept as the config changes (Host::new).
pub fn bar() -> i32 {
    HEIGHT.with(Cell::get)
}

fn height() -> i32 {
    crate::config::load().appearance.bar_height.clamp(16, 64) as i32
}

/// The bar's window: its mode (i3's bar mode toggle: docked, or hidden and shown over the windows while Super
/// is held), the launcher in it, the popup open over it.
pub struct Host {
    pub win: gtk4::ApplicationWindow,
    layer: gtk4::Overlay,
    pub docked: Cell<bool>,
    pub peeking: Cell<bool>,
    pub launching: Cell<bool>,
    /// its monitor's size, the window's from the start (its own known only once laid out)
    screen: Cell<(i32, i32)>,
    open: RefCell<Option<Weak<Popup>>>,
    grab: RefCell<Option<crate::wm::Grab>>,
    /// a slider's value over its knob while it moves (bubble)
    bubble: gtk4::Label,
}

impl Host {
    /// The bar's window over the whole of a monitor (the compositor's pick with None), strip (the bar) at its top.
    pub fn new(app: &gtk4::Application, strip: &impl IsA<gtk4::Widget>, monitor: Option<&gtk4::gdk::Monitor>) -> Rc<Host> {
        let win = gtk4::ApplicationWindow::new(app);
        win.init_layer_shell();
        win.set_namespace(Some("ostrov"));
        // the top and both sides, as tall as the screen: anchored to all four edges its strip would be ignored
        for e in [Edge::Top, Edge::Left, Edge::Right] {
            win.set_anchor(e, true);
        }
        win.set_monitor(monitor);
        let screen = size(monitor);
        let col = gtk4::Box::new(Orientation::Vertical, 0);
        strip.set_size_request(-1, bar());
        col.append(strip);
        // a new height in the config the strip's and the windows' strip at once, the popup open hung under it
        let s = strip.clone().upcast::<gtk4::Widget>();
        let me: Rc<RefCell<Weak<Host>>> = Rc::default();
        let m2 = me.clone();
        crate::style::on_config(move || {
            let now = height();
            if HEIGHT.with(|h| h.replace(now)) != now {
                s.set_size_request(-1, now);
                if let Some(h) = m2.borrow().upgrade() {
                    if let Some(p) = h.popup() {
                        p.reveal.set_margin_top(now);
                    }
                    h.apply();
                }
            }
        });
        let layer = gtk4::Overlay::new();
        layer.set_child(Some(&col));
        layer.set_size_request(-1, screen.1);
        win.set_child(Some(&layer));

        let host = Rc::new(Host {
            win: win.clone(),
            layer,
            docked: Cell::new(true),
            peeking: Cell::new(false),
            launching: Cell::new(false),
            screen: Cell::new(screen),
            open: RefCell::default(),
            grab: RefCell::default(),
            bubble: gtk4::Label::new(None),
        });
        *me.borrow_mut() = Rc::downgrade(&host);
        HOSTS.with(|h| h.borrow_mut().push(Rc::downgrade(&host)));

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
        hover_tips(&host);
        host.bubble.add_css_class("bubble");
        host.bubble.set_halign(Align::Start);
        host.bubble.set_valign(Align::Start);
        host.bubble.set_can_target(false);
        host.bubble.set_visible(false);
        host.layer.add_overlay(&host.bubble);
        let click = gtk4::GestureClick::new();
        let h = Rc::downgrade(&host);
        click.connect_released(move |_, _, _, y| {
            if let Some(p) = h.upgrade().and_then(|h| h.popup()).filter(|p| y >= bar() as f64 && !p.contains(y)) {
                p.close();
            }
        });
        win.add_controller(click);
        // without a grab (another compositor) a click into a window, caught by the keyboard it takes
        let h = Rc::downgrade(&host);
        win.connect_is_active_notify(move |w| {
            let Some(h) = h.upgrade() else { return };
            if !w.is_active() && h.grab.borrow().is_none()
                && let Some(p) = h.popup() {
                    p.close();
                }
        });
        // the input set once there is a surface to set it on
        let h = Rc::downgrade(&host);
        win.connect_map(move |_| {
            if let Some(h) = h.upgrade() {
                h.apply();
            }
        });
        host.apply();
        win.present();
        host
    }

    /// Moved to another monitor (one gone, the bars one fewer); whether it was elsewhere.
    pub fn set_monitor(&self, monitor: &gtk4::gdk::Monitor) -> bool {
        if self.win.monitor().as_ref() == Some(monitor) {
            return false;
        }
        self.win.set_monitor(Some(monitor));
        self.screen.set(size(Some(monitor)));
        self.layer.set_size_request(-1, self.screen.get().1);
        self.apply();
        true
    }

    /// Its monitor's name (eDP-1).
    pub fn connector(&self) -> Option<String> {
        self.win.monitor().and_then(|m| m.connector()).map(|c| c.to_string())
    }

    /// Gone with its monitor.
    pub fn destroy(&self) {
        HOSTS.with(|h| h.borrow_mut().retain(|w| w.upgrade().is_some_and(|h| !std::ptr::eq(&*h, self))));
        self.win.destroy();
    }

    /// A widget laid over the window, drawn but taking no input (the launcher's preview).
    pub fn overlay(&self, w: &impl IsA<gtk4::Widget>) {
        self.layer.add_overlay(w);
    }

    /// The popup open, if one is (here: not moved to another bar since).
    pub fn popup(&self) -> Option<Rc<Popup>> {
        let here = |p: &Rc<Popup>| std::ptr::eq(p.host.borrow().as_ptr(), self);
        self.open.borrow().as_ref().and_then(Weak::upgrade).filter(|p| p.is_open() && here(p))
    }

    /// The window set for its mode and what is open in it: its layer and strip, the keyboard (the launcher's
    /// alone; an open popup's on demand, Hyprland's focus grab handing it the keys: Tab and the arrows around its
    /// widgets, Escape out; taken alone, the compositor clears the grab and the popup closes at once), shown or
    /// not, input where it is drawn.
    pub fn apply(&self) {
        let popup = self.popup();
        if self.docked.get() {
            self.win.set_layer(Layer::Top);
            self.win.set_exclusive_zone(bar());
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
        let (sw, sh) = self.screen.get();
        let r = gtk4::cairo::Region::create_rectangle(&gtk4::cairo::RectangleInt::new(0, 0, sw, bar()));
        if let Some((x, w)) = column {
            let _ = r.union_rectangle(&gtk4::cairo::RectangleInt::new(x, bar(), w, sh - bar()));
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

/// Hovers drawn in the bar's window itself while a popup is open: GTK's tooltips are surfaces of their own, which
/// Hyprland's focus grab on the popup leaves unshown. What the pointer rests on 400 ms (or the nearest of its
/// parents with a tooltip) has its tooltip shown under the pointer, gone as it moves to something else.
fn hover_tips(host: &Rc<Host>) {
    let tip = gtk4::Label::new(None);
    tip.add_css_class("hover-tip");
    tip.set_halign(Align::Start);
    tip.set_valign(Align::Start);
    tip.set_can_target(false);
    tip.set_visible(false);
    tip.set_wrap(true);
    tip.set_max_width_chars(40);
    host.layer.add_overlay(&tip);
    let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let shown: Rc<RefCell<String>> = Rc::default();
    let motion = gtk4::EventControllerMotion::new();
    let (h, t, tm, sh) = (Rc::downgrade(host), tip.clone(), timer.clone(), shown.clone());
    motion.connect_motion(move |ec, x, y| {
        let Some(host) = h.upgrade() else { return };
        // none while a button is held (a slider dragged: its bubble says it), a popup's own: the bar alone gets
        // GTK's
        let held = ec.current_event_state().intersects(gtk4::gdk::ModifierType::BUTTON1_MASK | gtk4::gdk::ModifierType::BUTTON3_MASK);
        let text = host.popup().filter(|_| !held).and_then(|_| {
            let mut w = host.win.pick(x, y, gtk4::PickFlags::DEFAULT);
            while let Some(x) = w {
                if let Some(t) = x.tooltip_text().filter(|t| !t.is_empty()) {
                    return Some(t.to_string());
                }
                w = x.parent();
            }
            None
        });
        if let Some(id) = tm.borrow_mut().take() {
            id.remove();
        }
        let Some(text) = text else {
            t.set_visible(false);
            sh.borrow_mut().clear();
            return;
        };
        if t.is_visible() && *sh.borrow() == text {
            return;
        }
        t.set_visible(false);
        let (t2, sh2, tm2, win) = (t.clone(), sh.clone(), tm.clone(), host.win.clone());
        *tm.borrow_mut() = Some(glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
            tm2.borrow_mut().take();
            t2.set_text(&text);
            *sh2.borrow_mut() = text;
            // under the pointer, kept within the window; measured without its last place (its margins are in
            // its size)
            on_top(&t2);
            t2.set_margin_start(0);
            t2.set_margin_top(0);
            let (_, nat) = t2.preferred_size();
            let left = (x as i32 + 8).min(win.width() - nat.width() - 8).max(0);
            t2.set_margin_start(left);
            t2.set_margin_top(y as i32 + 20);
            t2.set_visible(true);
        }));
    });
    let (t, tm) = (tip.clone(), timer.clone());
    motion.connect_leave(move |_| {
        if let Some(id) = tm.borrow_mut().take() {
            id.remove();
        }
        t.set_visible(false);
    });
    host.win.add_controller(motion);
    // a press puts the hover away, whatever it lands on
    let press = gtk4::GestureClick::new();
    press.set_button(0);
    press.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let (t, tm) = (tip.clone(), timer.clone());
    press.connect_pressed(move |_, _, _, _| {
        if let Some(id) = tm.borrow_mut().take() {
            id.remove();
        }
        t.set_visible(false);
    });
    host.win.add_controller(press);
}

/// Over a widget in a bar's window (a slider's track), at frac of its width from its left, the text in a bubble;
/// None: the bubble gone. Drawn in the window itself, over what clips the widget (a tile), as the hovers are.
pub fn bubble(over: &gtk4::Widget, frac: f64, text: Option<&str>) {
    let hosts: Vec<Rc<Host>> = HOSTS.with(|h| h.borrow().iter().filter_map(Weak::upgrade).collect());
    let Some(root) = over.root() else { return };
    let Some(host) = hosts.into_iter().find(|h| h.win.upcast_ref::<gtk4::Widget>() == root.upcast_ref::<gtk4::Widget>()) else { return };
    let b = &host.bubble;
    let (Some(text), Some(at)) = (text, over.compute_bounds(&host.win)) else { return b.set_visible(false) };
    b.set_text(text);
    on_top(b);
    // measured shown (a hidden widget asks for nothing) and without its last place (its margins are in its size)
    b.set_visible(true);
    b.set_margin_start(0);
    b.set_margin_top(0);
    let (_, nat) = b.preferred_size();
    // the knob's travel inside the track's rounded ends
    let inset = (at.height() / 2.0).max(4.0) as f64;
    let x = (at.x() as f64 + inset + (at.width() as f64 - 2.0 * inset) * frac.clamp(0.0, 1.0)) as i32 - nat.width() / 2;
    b.set_margin_start(x.clamp(0, (host.win.width() - nat.width()).max(0)));
    b.set_margin_top((at.y() as i32 - nat.height() - 6).max(0));
    b.set_visible(true);
}

/// Drawn over the rest of its window: moved after the popups added to it since.
fn on_top(w: &impl IsA<gtk4::Widget>) {
    let Some(parent) = w.parent() else { return };
    if parent.last_child().as_ref() != Some(w.upcast_ref()) {
        w.insert_before(&parent, None::<&gtk4::Widget>);
    }
}

/// A monitor's size, a guess without one.
fn size(monitor: Option<&gtk4::gdk::Monitor>) -> (i32, i32) {
    monitor.map_or((1920, 1080), |m| (m.geometry().width(), m.geometry().height()))
}

/// The bar of the monitor focused, the first without a compositor that says.
pub fn focused() -> Option<Rc<Host>> {
    let hosts: Vec<Rc<Host>> = HOSTS.with(|h| h.borrow().iter().filter_map(Weak::upgrade).collect());
    let names: Vec<Option<String>> = hosts.iter().map(|h| h.connector()).collect();
    hosts.into_iter().nth(pick(&names, crate::wm::focused_monitor().as_deref()))
}

/// Of the bars' monitors' names, the focused one's place, else the first's.
fn pick(names: &[Option<String>], focused: Option<&str>) -> usize {
    focused.and_then(|f| names.iter().position(|n| n.as_deref() == Some(f))).unwrap_or(0)
}

/// Where the popup hangs: its left edge under its tab's (a panel at the bar's left), its right edge under its
/// tab's (the bar's right end, the tray), or in the middle.
#[derive(Clone, Copy, PartialEq)]
pub enum Side {
    Left,
    Right,
    Center,
}

/// How a popup on that side lines up under its tab.
fn align(side: Side) -> Align {
    match side {
        Side::Left => Align::Start,
        Side::Right => Align::End,
        Side::Center => Align::Center,
    }
}

/// The top edge's row for a side: the gap under the tab, an edge on the side the popup reaches past it.
fn edges(top: &gtk4::Box, [left, gap, right]: [&gtk4::Box; 3], side: Side) {
    gap.remove_css_class("gap");
    gap.remove_css_class("gap-mid");
    match side {
        Side::Left => {
            gap.add_css_class("gap");
            top.append(gap);
            top.append(right);
        }
        Side::Right => {
            gap.add_css_class("gap");
            top.append(left);
            top.append(gap);
        }
        Side::Center => {
            gap.add_css_class("gap-mid");
            top.append(left);
            top.append(gap);
            top.append(right);
        }
    }
}

pub struct Popup {
    host: RefCell<Weak<Host>>,
    reveal: gtk4::Revealer,
    shape: gtk4::Box,
    /// the bar's block it grows out of; the tray's menu changes it to the icon clicked
    tab: RefCell<gtk4::Widget>,
    side: Cell<Side>,
    /// its top edge's row, and the edges at the tab's left and right, laid out by side
    top: gtk4::Box,
    edges: [gtk4::Box; 2],
    gap: gtk4::Box,
    on_open: RefCell<Vec<Box<dyn Fn()>>>,
    closed: Cell<Option<std::time::Instant>>,
    /// its block in every bar, a panel's: where a command opens it
    tabs: RefCell<Vec<(Weak<Host>, gtk4::Widget)>>,
}

impl Popup {
    /// A popup of width (-1: its own) hanging from tab, its body (a box of class surface) inside.
    pub fn new(host: &Rc<Host>, tab: &impl IsA<gtk4::Widget>, side: Side, width: i32, body: &gtk4::Box) -> Rc<Popup> {
        let reveal = gtk4::Revealer::new();
        reveal.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        reveal.set_transition_duration(120);
        reveal.set_valign(Align::Start);
        reveal.set_halign(align(side));
        reveal.set_margin_top(bar());
        reveal.set_size_request(width, -1);

        // the shape: a top edge from the corners to the tab, open under it, then the body without a top edge
        let shape = gtk4::Box::new(Orientation::Vertical, 0);
        let top = gtk4::Box::new(Orientation::Horizontal, 0);
        let left = gtk4::Box::new(Orientation::Horizontal, 0);
        left.add_css_class("edge");
        left.set_hexpand(true);
        let gap = gtk4::Box::new(Orientation::Horizontal, 0);
        let right = gtk4::Box::new(Orientation::Horizontal, 0);
        right.add_css_class("edge-right");
        right.set_hexpand(true);
        edges(&top, [&left, &gap, &right], side);
        shape.append(&top);
        body.add_css_class("attached");
        shape.append(body);
        reveal.set_child(Some(&shape));
        host.layer.add_overlay(&reveal);

        let popup = Rc::new(Popup {
            host: RefCell::new(Rc::downgrade(host)),
            reveal: reveal.clone(),
            shape,
            tab: RefCell::new(tab.clone().upcast()),
            side: Cell::new(side),
            top,
            edges: [left, right],
            gap,
            on_open: RefCell::default(),
            closed: Cell::default(),
            tabs: RefCell::default(),
        });
        // rolled up: the tab a block again, the input back to the strip
        let p = Rc::downgrade(&popup);
        reveal.connect_child_revealed_notify(move |r| {
            let Some(p) = p.upgrade().filter(|_| !r.is_child_revealed() && !r.reveals_child()) else { return };
            p.tab.borrow().remove_css_class("tab");
            if let Some(h) = p.host().filter(|h| h.popup().is_none()) {
                h.closed();
            }
        });
        popup
    }

    pub fn is_open(&self) -> bool {
        self.reveal.reveals_child()
    }

    /// The bar's window it is in.
    pub fn host(&self) -> Option<Rc<Host>> {
        self.host.borrow().upgrade()
    }

    /// Its column in the window: x and width.
    fn column(&self) -> (i32, i32) {
        let Some(b) = self.host().and_then(|h| self.reveal.compute_bounds(&h.win)) else { return (0, 0) };
        (b.x() as i32, b.width() as i32)
    }

    /// y in the window within the popup's shape.
    fn contains(&self, y: f64) -> bool {
        y < bar() as f64 + self.shape.height() as f64
    }

    /// f on every opening (the panel folds its menus, the calendar goes back to this month).
    pub fn on_open(&self, f: impl Fn() + 'static) {
        self.on_open.borrow_mut().push(Box::new(f));
    }

    /// A bar's block it may hang from: a command opens it under the focused monitor's.
    pub fn add_tab(&self, host: &Rc<Host>, tab: &impl IsA<gtk4::Widget>) {
        self.tabs.borrow_mut().push((Rc::downgrade(host), tab.clone().upcast()));
    }

    /// Hung from a tab in another bar's window: closed in the one it was in, laid over this one.
    fn hang(&self, host: &Rc<Host>, tab: &gtk4::Widget) {
        let old = self.host();
        if !old.as_ref().is_some_and(|o| Rc::ptr_eq(o, host)) {
            if self.is_open() {
                self.tab.borrow().remove_css_class("tab");
                self.reveal.set_reveal_child(false);
                if let Some(o) = &old {
                    o.closed();
                }
            }
            if let Some(o) = self.reveal.parent().and_downcast::<gtk4::Overlay>() {
                o.remove_overlay(&self.reveal);
            }
            host.layer.add_overlay(&self.reveal);
            *self.host.borrow_mut() = Rc::downgrade(host);
        }
        self.set_tab(tab);
    }

    /// Opened, any other popup closed; closed, under the focused monitor's tab if it has one there.
    pub fn open(self: &Rc<Self>) {
        let here = crate::popup::focused().and_then(|f| {
            let tabs = self.tabs.borrow();
            tabs.iter().find(|(h, _)| h.upgrade().is_some_and(|h| Rc::ptr_eq(&h, &f))).map(|(_, t)| (f, t.clone()))
        });
        if let Some((h, t)) = here.filter(|_| !self.is_open()) {
            self.hang(&h, &t);
        }
        self.show();
    }

    /// Opened where it hangs, any other popup there closed.
    fn show(self: &Rc<Self>) {
        let Some(host) = self.host() else { return };
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
        // the keys start at its first widget (ringed only once a key is pressed, GTK's focus-visible)
        let shape = self.shape.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(s) = shape.upgrade() {
                s.child_focus(gtk4::DirectionType::TabForward);
            }
        });
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
        let Some(host) = self.host() else { return };
        self.reveal.set_margin_top(bar());
        let tab = self.tab.borrow();
        let Some(b) = tab.compute_bounds(&host.win) else { return };
        let inner = if self.side.get() == Side::Center { 2 } else { 1 };
        let tw = b.width().round() as i32;
        if tw > inner {
            self.gap.set_size_request(tw - inner, -1);
        }
        match self.side.get() {
            Side::Right => self.reveal.set_margin_end(host.win.width() - (b.x() + b.width()).round() as i32),
            Side::Left => self.reveal.set_margin_start(b.x().round() as i32),
            Side::Center => {}
        }
    }

    /// Hung on another side of its tab, its block moved to another part of the bar (bar/edit.rs).
    pub fn set_side(&self, side: Side) {
        if self.side.replace(side) == side {
            return;
        }
        self.reveal.set_halign(align(side));
        self.reveal.set_margin_start(0);
        self.reveal.set_margin_end(0);
        let [left, right] = &self.edges;
        for w in [left, &self.gap, right] {
            if w.parent().is_some() {
                self.top.remove(w);
            }
        }
        edges(&self.top, [left, &self.gap, right], side);
        if self.is_open() {
            self.place();
        }
    }

    /// As wide as that (a panel's grid made wider or narrower).
    pub fn set_width(&self, width: i32) {
        self.reveal.set_size_request(width, -1);
        if self.is_open() {
            self.place();
        }
    }

    /// Taken out of its bar's window for good (the bar's editor's gallery, done).
    pub fn remove(&self) {
        if let Some(o) = self.reveal.parent().and_downcast::<gtk4::Overlay>() {
            o.remove_overlay(&self.reveal);
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
        } else if !self.just_closed() {
            self.open()
        }
    }

    /// Toggled by a click on one of its tabs: opened under it, from another bar's moved there.
    pub fn toggle_at(self: &Rc<Self>, host: &Rc<Host>, tab: &impl IsA<gtk4::Widget>) {
        let tab = tab.clone().upcast::<gtk4::Widget>();
        if self.tab() != tab {
            self.hang(host, &tab);
            self.show();
        } else if self.is_open() {
            self.close()
        } else if !self.just_closed() {
            self.show()
        }
    }

    fn just_closed(&self) -> bool {
        self.closed.get().is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(300))
    }
}

#[cfg(test)]
mod tests {
    use super::pick;

    #[test]
    fn a_command_opens_on_the_focused_monitor() {
        let names = [Some("eDP-1".to_string()), Some("DP-2".to_string()), None];
        assert_eq!(pick(&names, Some("DP-2")), 1);
        assert_eq!(pick(&names, Some("eDP-1")), 0);
        // a monitor without a bar, no compositor saying, no bar: the first
        assert_eq!(pick(&names, Some("HDMI-A-1")), 0);
        assert_eq!(pick(&names, None), 0);
        assert_eq!(pick(&[], Some("DP-2")), 0);
    }
}
