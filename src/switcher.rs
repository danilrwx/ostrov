//! The window switcher, held like Alt+Tab: `ostrov windows` (Super+Tab) opens it over the screen's middle with
//! Hyprland's windows as cards, the last focused first, the one before the current picked; Super+Tab again, Tab,
//! Right or Ctrl+N move on, Shift+Tab, Left or Ctrl+P back; Super let go, Enter or a click focuses the picked
//! window, Escape leaves things as they were. `ostrov windows app` (Super+`) the same over the focused app's
//! windows alone. Super's release is Hyprland's to tell (ostrov windows release, a transparent release bind: GTK
//! has no word of Super held before the switcher had the keyboard). Shown only once Super has been held a
//! moment: a quick tap goes straight to the window before, nothing flashing up, GNOME's way.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk::{Key, ModifierType};
use gtk4::prelude::*;
use gtk4::{gio, glib, Align, Orientation};
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};
use serde_json::Value;

use crate::style::label;

/// A window, as the switcher shows it.
struct Win {
    address: String,
    class: String,
    title: String,
    workspace: String,
}

/// Hyprland's windows (j/clients) the last focused first; the unmapped and a group's hidden ones left out.
fn windows(clients: &str) -> Vec<Win> {
    let list: Value = serde_json::from_str(clients).unwrap_or_default();
    let mut wins: Vec<(i64, Win)> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["mapped"] != false && c["hidden"] != true)
        .map(|c| {
            let s = |v: &Value| v.as_str().unwrap_or_default().to_string();
            let win = Win {
                address: s(&c["address"]),
                class: s(&c["class"]),
                title: s(&c["title"]),
                workspace: s(&c["workspace"]["name"]),
            };
            (c["focusHistoryID"].as_i64().unwrap_or(i64::MAX), win)
        })
        .collect();
    wins.sort_by_key(|w| w.0);
    wins.into_iter().map(|w| w.1).collect()
}

/// A window's app icon: its class's desktop file ("<class>.desktop", as is or lowercased), else the app whose
/// StartupWMClass is the class.
pub(crate) fn icon(class: &str) -> Option<gio::Icon> {
    let by_name = [class.to_string(), class.to_lowercase()]
        .into_iter()
        .find_map(|c| gio_unix::DesktopAppInfo::new(&format!("{c}.desktop")));
    let app = by_name.or_else(|| {
        gio::AppInfo::all().into_iter().filter_map(|a| a.downcast::<gio_unix::DesktopAppInfo>().ok()).find(|d| {
            d.startup_wm_class().is_some_and(|w| w.eq_ignore_ascii_case(class))
        })
    })?;
    app.icon()
}

pub struct Switcher {
    win: gtk4::ApplicationWindow,
    grid: gtk4::Grid,
    wins: RefCell<Vec<Win>>,
    picked: Cell<usize>,
    /// Super let go with the switcher not up yet: its tap's release come before its opening
    released: Cell<Option<std::time::Instant>>,
}

impl Switcher {
    pub fn new(app: &gtk4::Application) -> Rc<Switcher> {
        let win = gtk4::ApplicationWindow::new(app);
        win.init_layer_shell();
        win.set_layer(Layer::Overlay);
        win.set_namespace(Some("ostrov-switcher"));
        win.set_keyboard_mode(KeyboardMode::Exclusive);
        let grid = gtk4::Grid::new();
        grid.add_css_class("surface");
        grid.set_row_spacing(8);
        grid.set_column_spacing(8);
        win.set_child(Some(&grid));
        let s = Rc::new(Switcher { win: win.clone(), grid, wins: RefCell::default(), picked: Cell::new(0), released: Cell::new(None) });

        // in the capture phase: Tab would move GTK's focus first
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let me = Rc::downgrade(&s);
        keys.connect_key_pressed(move |_, k, _, m| {
            let Some(me) = me.upgrade() else { return glib::Propagation::Proceed };
            let ctrl = m.contains(ModifierType::CONTROL_MASK);
            match k {
                Key::Tab | Key::Right => me.step(1),
                Key::n | Key::N if ctrl => me.step(1),
                Key::ISO_Left_Tab | Key::Left => me.step(-1),
                Key::p | Key::P if ctrl => me.step(-1),
                Key::Return | Key::KP_Enter => me.pick(me.picked.get()),
                Key::Escape => me.close(),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        let me = Rc::downgrade(&s);
        keys.connect_key_released(move |_, k, _, _| {
            if let Some(me) = me.upgrade().filter(|_| matches!(k, Key::Super_L | Key::Super_R)) {
                me.pick(me.picked.get());
            }
        });
        win.add_controller(keys);

        s
    }

    /// Open with the windows as they are now, the one before the current picked; open, the next picked. With
    /// app (Super+`), the focused window's app's windows alone: a browser's, a terminal's, GNOME's Alt+`.
    pub fn open(self: &Rc<Self>, app: bool) {
        if self.win.is_visible() {
            return self.step(1);
        }
        let mut wins = windows(&crate::wm::hyprctl("j/clients"));
        if app {
            let class = wins.first().map(|w| w.class.clone()).unwrap_or_default();
            wins.retain(|w| w.class == class);
        }
        if wins.len() < 2 && app || wins.is_empty() {
            return;
        }
        crate::style::clear(&self.grid);
        // a row, or rows of six
        for (n, w) in wins.iter().enumerate() {
            let card = gtk4::Box::new(Orientation::Vertical, 6);
            card.add_css_class("card");
            card.add_css_class("switch");
            card.set_size_request(150, -1);
            let img = match icon(&w.class) {
                Some(i) => gtk4::Image::from_gicon(&i),
                None => gtk4::Image::from_icon_name("application-x-executable"),
            };
            img.set_pixel_size(48);
            card.append(&img);
            let title = label(&w.title, "");
            title.set_halign(Align::Center);
            title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            title.set_max_width_chars(18);
            card.append(&title);
            let ws = label(&w.workspace, "dim");
            ws.set_halign(Align::Center);
            card.append(&ws);
            card.set_cursor_from_name(Some("pointer"));
            let click = gtk4::GestureClick::new();
            let me = Rc::downgrade(self);
            click.connect_released(move |_, _, _, _| {
                if let Some(me) = me.upgrade() {
                    me.pick(n)
                }
            });
            card.add_controller(click);
            self.grid.attach(&card, (n % 6) as i32, (n / 6) as i32, 1, 1);
        }
        let first = if wins.len() > 1 { 1 } else { 0 };
        *self.wins.borrow_mut() = wins;
        self.picked.set(0);
        self.set_picked(first);
        // a tap whose release came first: the window before, nothing shown
        if self.released.take().is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(400)) {
            self.win.set_visible(true);
            return self.pick(first);
        }
        // up unseen, for the keyboard; seen once Super has been held 150 ms, unless let go before
        self.grid.set_opacity(0.0);
        self.win.present();
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
            if let Some(me) = me.upgrade() {
                me.grid.set_opacity(1.0);
            }
        });
        // the cards clickable at once: Hyprland hands the pointer to a new surface only on a motion
        crate::wm::nudge();
    }

    /// Super let go (Hyprland's release bind): the picked window, or, the switcher not up yet, kept for its
    /// opening.
    pub fn release(&self) {
        if self.win.is_visible() {
            self.pick(self.picked.get());
        } else {
            self.released.set(Some(std::time::Instant::now()));
        }
    }

    fn card(&self, n: usize) -> Option<gtk4::Widget> {
        self.grid.child_at((n % 6) as i32, (n / 6) as i32)
    }

    fn set_picked(&self, n: usize) {
        if let Some(c) = self.card(self.picked.get()) {
            c.remove_css_class("picked");
        }
        self.picked.set(n);
        if let Some(c) = self.card(n) {
            c.add_css_class("picked");
        }
    }

    /// The picked one moved by a step, round the ends.
    fn step(&self, by: isize) {
        let len = self.wins.borrow().len() as isize;
        if len > 0 {
            self.set_picked((self.picked.get() as isize + by).rem_euclid(len) as usize);
        }
    }

    /// Gone, the pointer left alone: moved, it would focus the window under it (follow_mouse), not the picked.
    fn close(&self) {
        self.win.set_visible(false);
    }

    /// The window focused, once the switcher is gone: Hyprland gives the keyboard back to the window it had as
    /// the layer goes, which would undo a focus asked before; asked again once that is surely over.
    fn pick(&self, n: usize) {
        if !self.win.is_visible() {
            return;
        }
        let address = self.wins.borrow().get(n).map(|w| w.address.clone());
        self.close();
        if let Some(a) = address {
            for ms in [30, 200] {
                let a = a.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || {
                    drop(crate::wm::hyprctl(&format!("dispatch focuswindow address:{a}")));
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_focused_first() {
        let json = r#"[
            {"address":"0xa","mapped":true,"hidden":false,"class":"kitty","title":"a","workspace":{"name":"2"},"focusHistoryID":2},
            {"address":"0xb","mapped":true,"hidden":false,"class":"firefox","title":"b","workspace":{"name":"1"},"focusHistoryID":0},
            {"address":"0xc","mapped":false,"hidden":false,"class":"x","title":"c","workspace":{"name":"1"},"focusHistoryID":1},
            {"address":"0xd","mapped":true,"hidden":true,"class":"x","title":"d","workspace":{"name":"1"},"focusHistoryID":3},
            {"address":"0xe","mapped":true,"hidden":false,"class":"Code","title":"e","workspace":{"name":"3"},"focusHistoryID":1}
        ]"#;
        let w: Vec<String> = windows(json).into_iter().map(|w| w.address).collect();
        assert_eq!(w, ["0xb", "0xe", "0xa"]);
        assert!(windows("").is_empty());
    }
}
