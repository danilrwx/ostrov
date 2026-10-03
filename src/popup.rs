//! A popup grown out of a block of the bar: while it is open the block is a tab (its ground and white sides,
//! no bottom), and the popup sits right under the bar with its top edge running from its corners to the tab and
//! open under it, so tab and popup are one shape. The window runs down to the screen's bottom and is never
//! resized (a layer surface resized frame by frame while a menu slides open jerks): the popup grows inside it,
//! a click in it beside or under the popup closing it. It has the keyboard from its opening, so a click into a
//! window, taking the keyboard, closes it too; so do the tab clicked again and Escape.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

/// Where the popup hangs: at the bar's right end, or in its middle.
#[derive(Clone, Copy, PartialEq)]
pub enum Side {
    Right,
    Center,
}

pub struct Popup {
    win: gtk4::ApplicationWindow,
    /// the popup unrolling down out of its tab as it opens, rolling back up as it closes, GNOME's way and short
    reveal: gtk4::Revealer,
    tab: gtk4::Widget,
    on_open: RefCell<Vec<Box<dyn Fn()>>>,
    closed: std::cell::Cell<Option<std::time::Instant>>,
}

impl Popup {
    /// A popup of width hanging from tab, its body (a box of class panel) inside; namespace is its layer's name
    /// (Hyprland blurs under it by that).
    pub fn new(
        app: &gtk4::Application,
        namespace: &str,
        tab: &impl IsA<gtk4::Widget>,
        side: Side,
        width: i32,
        body: &gtk4::Box,
    ) -> Rc<Popup> {
        let win = gtk4::ApplicationWindow::new(app);
        win.init_layer_shell();
        win.set_layer(Layer::Overlay);
        win.set_namespace(Some(namespace));
        win.set_anchor(Edge::Top, true);
        win.set_anchor(Edge::Bottom, true);
        if side == Side::Right {
            win.set_anchor(Edge::Right, true);
            win.set_margin(Edge::Right, 6);
        }
        // the keyboard on demand: Hyprland gives it on the opening, so a passphrase, Escape, and its loss
        win.set_keyboard_mode(KeyboardMode::OnDemand);
        win.set_default_size(width, -1);
        win.add_css_class("panel-window");

        let reveal = gtk4::Revealer::new();
        reveal.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        reveal.set_transition_duration(120);
        reveal.set_valign(Align::Start);
        let popup = Rc::new(Popup { win: win.clone(), reveal: reveal.clone(), tab: tab.clone().upcast(), on_open: RefCell::default(), closed: Default::default() });

        let keys = gtk4::EventControllerKey::new();
        let p = popup.clone();
        keys.connect_key_pressed(move |_, k, _, _| {
            if k == gtk4::gdk::Key::Escape {
                p.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        win.add_controller(keys);

        // the shape: a top edge from the corners to the tab, open under it, then the body without a top edge
        let shape = gtk4::Box::new(Orientation::Vertical, 0);
        shape.set_valign(Align::Start);
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
        win.set_child(Some(&reveal));
        // unrolled once the window is on screen (a revealer not yet mapped would just jump); rolled up, gone
        win.connect_map(move |w| {
            if let Some(r) = w.child().and_downcast::<gtk4::Revealer>() {
                r.set_reveal_child(true);
            }
        });
        let p = Rc::downgrade(&popup);
        reveal.connect_child_revealed_notify(move |r| {
            if let Some(p) = p.upgrade().filter(|_| !r.is_child_revealed() && !r.reveals_child()) {
                p.win.set_visible(false);
                p.tab.remove_css_class("tab");
                nudge();
            }
        });

        // on every opening the gap as wide as the tab's border box (width() is its content alone)
        let tab2 = popup.tab.clone();
        let inner = if side == Side::Right { 1 } else { 2 };
        win.connect_map(move |_| {
            let tw = tab2.compute_bounds(&tab2).map(|b| b.width().round() as i32).unwrap_or(0);
            if tw > inner {
                gap.set_size_request(tw - inner, -1);
            }
        });
        // the window runs down to the screen's bottom: a click in it under the popup, or beside it, closes it
        let click = gtk4::GestureClick::new();
        let p = Rc::downgrade(&popup);
        click.connect_released(move |g, _, x, y| {
            let (Some(p), Some(w)) = (p.upgrade(), g.widget()) else { return };
            let hit = w.pick(x, y, gtk4::PickFlags::DEFAULT);
            if hit.is_none_or(|h| h == w || h == p.reveal.clone().upcast::<gtk4::Widget>()) {
                p.close();
            }
        });
        win.add_controller(click);
        // a click into a window, caught by the keyboard it takes: the popup has it from its opening (on demand),
        // and closes as it loses it
        let p = Rc::downgrade(&popup);
        win.connect_is_active_notify(move |w| {
            if let Some(p) = p.upgrade().filter(|p| !w.is_active() && p.is_open()) {
                p.close();
            }
        });
        popup
    }

    pub fn is_open(&self) -> bool {
        self.win.is_visible() && self.reveal.reveals_child()
    }

    /// f on every opening (the panel folds its menus, the calendar goes back to this month).
    pub fn on_open(&self, f: impl Fn() + 'static) {
        self.on_open.borrow_mut().push(Box::new(f));
    }

    pub fn open(&self) {
        for f in self.on_open.borrow().iter() {
            f();
        }
        self.tab.add_css_class("tab");
        if self.win.is_visible() {
            // opened again while rolling up
            self.reveal.set_reveal_child(true);
        } else {
            self.win.set_visible(true);
        }
    }

    pub fn close(&self) {
        if self.reveal.reveals_child() {
            self.closed.set(Some(std::time::Instant::now()));
        }
        self.reveal.set_reveal_child(false);
    }

    /// Open or close; closed a moment ago, it stays closed: the click on its tab that toggles it is the one
    /// that took the keyboard from it and closed it just before.
    pub fn toggle(&self) {
        if self.is_open() {
            self.close()
        } else if !self.closed.get().is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(300)) {
            self.open()
        }
    }
}

/// The pointer put back where it is, once the layers have mapped or gone: Hyprland hands the pointer to a surface
/// that maps under it only on its next motion, so a click without one would go nowhere.
fn nudge() {
    glib::timeout_add_local_once(std::time::Duration::from_millis(30), || {
        crate::hub::run(&["sh", "-c", "hyprctl dispatch movecursor $(hyprctl cursorpos | tr -d ,)"]);
    });
}
