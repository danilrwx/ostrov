//! A popup grown out of a block of the bar: while it is open the block is a tab (its ground and white sides,
//! no bottom), and the popup sits right under the bar with its top edge running from its corners to the tab and
//! open under it, so tab and popup are one shape. The window runs down to the screen's bottom and is never
//! resized (a layer surface resized frame by frame while a menu slides open jerks): the popup grows inside it,
//! taking input only over itself. A click outside it (a see-through catcher over the whole screen,
//! the bar too: the tab clicked again closes it) or Escape closes it.

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
    catcher: gtk4::ApplicationWindow,
    tab: gtk4::Widget,
    on_open: RefCell<Vec<Box<dyn Fn()>>>,
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
        let catcher = gtk4::ApplicationWindow::new(app);
        catcher.init_layer_shell();
        catcher.set_layer(Layer::Top);
        catcher.set_namespace(Some("rbar-catcher"));
        for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            catcher.set_anchor(e, true);
        }
        catcher.set_exclusive_zone(-1);
        catcher.add_css_class("catcher");

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
        // the keyboard on a click into it: a passphrase, Escape
        win.set_keyboard_mode(KeyboardMode::OnDemand);
        win.set_default_size(width, -1);
        win.add_css_class("panel-window");

        let popup = Rc::new(Popup { win: win.clone(), catcher: catcher.clone(), tab: tab.clone().upcast(), on_open: RefCell::default() });

        let click = gtk4::GestureClick::new();
        let p = popup.clone();
        // on the release: the press and the release both the catcher's, none left for the bar to reopen it with
        click.connect_released(move |_, _, _, _| p.close());
        catcher.add_controller(click);
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
        win.set_child(Some(&shape));

        // every frame: the gap as wide as the tab's border box (width() is its content alone), the input region
        // over the popup as it grows and shrinks
        let last = Rc::new(RefCell::new((0, 0)));
        // a surface anew on every mapping, its input region the whole of it again: set it anew
        let l2 = last.clone();
        win.connect_map(move |_| *l2.borrow_mut() = (0, 0));
        let (w2, tab2) = (win.clone(), popup.tab.clone());
        let inner = if side == Side::Right { 1 } else { 2 };
        shape.add_tick_callback(move |shape, _| {
            let tw = tab2.compute_bounds(&tab2).map(|b| b.width().round() as i32).unwrap_or(0);
            if tw > inner && gap.width_request() != tw - inner {
                gap.set_size_request(tw - inner, -1);
            }
            let (w, h) = (shape.width(), shape.height());
            if *last.borrow() != (w, h) {
                *last.borrow_mut() = (w, h);
                if let Some(surface) = w2.surface() {
                    let rect = gtk4::cairo::RectangleInt::new(0, 0, w, h);
                    surface.set_input_region(Some(&gtk4::cairo::Region::create_rectangle(&rect)));
                }
            }
            glib::ControlFlow::Continue
        });
        popup
    }

    pub fn is_open(&self) -> bool {
        self.win.is_visible()
    }

    /// f on every opening (the panel folds its menus, the calendar goes back to this month).
    pub fn on_open(&self, f: impl Fn() + 'static) {
        self.on_open.borrow_mut().push(Box::new(f));
    }

    pub fn open(&self) {
        for f in self.on_open.borrow().iter() {
            f();
        }
        self.catcher.set_visible(true);
        self.win.set_visible(true);
        self.tab.add_css_class("tab");
        nudge();
    }

    pub fn close(&self) {
        self.win.set_visible(false);
        self.catcher.set_visible(false);
        self.tab.remove_css_class("tab");
        nudge();
    }

    pub fn toggle(&self) {
        if self.is_open() { self.close() } else { self.open() }
    }
}

/// The pointer put back where it is, once the layers have mapped or gone: Hyprland hands the pointer to a surface
/// that maps under it only on its next motion, so a click without one would go nowhere.
fn nudge() {
    glib::timeout_add_local_once(std::time::Duration::from_millis(30), || {
        crate::hub::run(&["sh", "-c", "hyprctl dispatch movecursor $(hyprctl cursorpos | tr -d ,)"]);
    });
}
