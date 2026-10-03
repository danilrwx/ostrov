//! The launcher in the bar, dmenu's way: in place of the clock a prompt, what is typed, the apps it matches in a
//! row (or the clipboard's history, from cliphist), the picked one inverted. Left, Right and Tab move, Enter
//! launches (or copies back), Delete drops a clipboard entry, Escape closes. ostrov run ($mod+d), ostrov clip
//! ($mod+Shift+v).

use std::cell::{Cell, RefCell};
use std::process::Command;
use std::rc::Rc;

use gtk4::gdk::Key;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use gtk4::{gio, glib, Orientation};

use crate::hub::run;

/// A hit: what the row shows, and what picking it does.
enum Hit {
    App(gio::AppInfo),
    Clip(String),
}

impl Hit {
    fn name(&self) -> String {
        match self {
            Hit::App(a) => a.name().to_string(),
            Hit::Clip(line) => line.split_once('\t').map_or(line.as_str(), |l| l.1).split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect(),
        }
    }
}

pub struct Launcher {
    pub widget: gtk4::Box,
    prompt: gtk4::Label,
    query: gtk4::Text,
    clip: Cell<bool>,
    all: RefCell<Vec<Hit>>,
    hits: RefCell<Vec<usize>>,
    picked: Cell<usize>,
    row: gtk4::Box,
    scroll: gtk4::ScrolledWindow,
    on_toggle: Box<dyn Fn(bool)>,
}

impl Launcher {
    /// on_toggle(open): the bar makes room for it and gives it the keyboard.
    pub fn new(on_toggle: impl Fn(bool) + 'static) -> Rc<Launcher> {
        let widget = gtk4::Box::new(Orientation::Horizontal, 10);
        widget.set_hexpand(true);
        widget.set_visible(false);
        widget.set_margin_start(12);
        let prompt = gtk4::Label::new(None);
        prompt.add_css_class("dim");
        let query = gtk4::Text::new();
        query.add_css_class("query");
        query.set_width_chars(24);
        query.set_max_width_chars(24);
        let row = gtk4::Box::new(Orientation::Horizontal, 0);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::External, gtk4::PolicyType::Never);
        scroll.set_hexpand(true);
        scroll.set_child(Some(&row));
        widget.append(&prompt);
        widget.append(&query);
        widget.append(&scroll);

        let l = Rc::new(Launcher {
            widget,
            prompt,
            query,
            clip: Cell::new(false),
            all: RefCell::default(),
            hits: RefCell::default(),
            picked: Cell::new(0),
            row,
            scroll,
            on_toggle: Box::new(on_toggle),
        });
        let l2 = l.clone();
        l.query.connect_changed(move |_| {
            l2.picked.set(0);
            l2.filter();
        });
        let keys = gtk4::EventControllerKey::new();
        let l2 = l.clone();
        keys.connect_key_pressed(move |_, k, _, m| {
            let n = l2.hits.borrow().len();
            let back = m.contains(gtk4::gdk::ModifierType::SHIFT_MASK);
            match k {
                Key::Escape => l2.close(),
                Key::Return | Key::KP_Enter => l2.pick(l2.picked.get()),
                Key::Delete if l2.clip.get() => l2.delete(),
                Key::Right | Key::Tab if n > 0 && !back => l2.set_picked((l2.picked.get() + 1) % n),
                Key::Left | Key::ISO_Left_Tab | Key::Tab if n > 0 => l2.set_picked((l2.picked.get() + n - 1) % n),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        l.query.add_controller(keys);
        l
    }

    pub fn is_open(&self) -> bool {
        self.widget.is_visible()
    }

    /// Open over the apps, or the clipboard's history; the same again closes it.
    pub fn toggle(self: &Rc<Self>, clip: bool) {
        if self.is_open() && self.clip.get() == clip {
            return self.close();
        }
        self.clip.set(clip);
        self.prompt.set_text(if clip { "clip" } else { "run" });
        *self.all.borrow_mut() = if clip {
            Command::new("cliphist")
                .arg("list")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.is_empty()).map(|l| Hit::Clip(l.into())).collect())
                .unwrap_or_default()
        } else {
            gio::AppInfo::all().into_iter().filter(|a| a.should_show()).map(Hit::App).collect()
        };
        self.query.set_text("");
        self.picked.set(0);
        self.filter();
        self.widget.set_visible(true);
        (self.on_toggle)(true);
        self.query.grab_focus();
    }

    pub fn close(&self) {
        self.widget.set_visible(false);
        (self.on_toggle)(false);
    }

    /// The hits for what is typed: apps by name, a name starting with it first; clips newest first.
    fn filter(self: &Rc<Self>) {
        let q = self.query.text().to_lowercase();
        let all = self.all.borrow();
        let mut hits: Vec<usize> = (0..all.len()).filter(|&i| all[i].name().to_lowercase().contains(&q)).collect();
        if !self.clip.get() {
            hits.sort_by_key(|&i| {
                let n = all[i].name();
                (!n.to_lowercase().starts_with(&q), n.to_lowercase())
            });
        }
        *self.hits.borrow_mut() = hits;
        drop(all);
        self.draw();
    }

    fn draw(self: &Rc<Self>) {
        crate::style::clear(&self.row);
        let all = self.all.borrow();
        // the first hundred: past that no one tabs
        for (n, &i) in self.hits.borrow().iter().take(100).enumerate() {
            let l = gtk4::Label::new(Some(&all[i].name()));
            l.add_css_class("hit");
            if n == self.picked.get() {
                l.add_css_class("picked");
            }
            l.set_cursor_from_name(Some("pointer"));
            let click = gtk4::GestureClick::new();
            let me = Rc::downgrade(self);
            click.connect_released(move |_, _, _, _| {
                if let Some(me) = me.upgrade() {
                    me.pick(n)
                }
            });
            l.add_controller(click);
            self.row.append(&l);
        }
    }

    /// The picked one inverted and scrolled into the row's view.
    fn set_picked(&self, n: usize) {
        if let Some(old) = self.row.observe_children().item(self.picked.get() as u32) {
            old.downcast::<gtk4::Widget>().unwrap().remove_css_class("picked");
        }
        self.picked.set(n);
        let Some(w) = self.row.observe_children().item(n as u32).and_then(|o| o.downcast::<gtk4::Widget>().ok()) else { return };
        w.add_css_class("picked");
        let Some(b) = w.compute_bounds(&self.row) else { return };
        let adj = self.scroll.hadjustment();
        let (x, end) = (b.x() as f64, (b.x() + b.width()) as f64);
        if x < adj.value() {
            adj.set_value(x);
        } else if end > adj.value() + adj.page_size() {
            adj.set_value(end - adj.page_size());
        }
    }

    /// An app launched (a terminal one in alacritty), a clip copied back; with none, what was typed run.
    fn pick(&self, n: usize) {
        let typed = self.query.text().to_string();
        let i = self.hits.borrow().get(n).copied();
        self.close();
        let all = self.all.borrow();
        match i.map(|i| &all[i]) {
            Some(Hit::App(a)) => {
                let term = a.downcast_ref::<gio_unix::DesktopAppInfo>().is_some_and(|d| d.boolean("Terminal"));
                if term {
                    let cmd = a.commandline().map(|c| c.to_string_lossy().into_owned()).unwrap_or_default();
                    let cmd: Vec<&str> = cmd.split_whitespace().filter(|w| !w.starts_with('%')).collect();
                    run(&[&["alacritty", "-e"], &cmd[..]].concat());
                } else {
                    let ctx = gtk4::gdk::Display::default().map(|d| d.app_launch_context());
                    let _ = a.launch(&[], ctx.as_ref());
                }
            }
            Some(Hit::Clip(line)) => run(&["sh", "-c", "printf '%s' \"$1\" | cliphist decode | wl-copy", "sh", line]),
            None if !typed.trim().is_empty() => run(&["sh", "-c", &typed]),
            None => {}
        }
    }

    /// The picked clip out of the history.
    fn delete(self: &Rc<Self>) {
        let Some(i) = self.hits.borrow().get(self.picked.get()).copied() else { return };
        if let Hit::Clip(line) = &self.all.borrow()[i] {
            run(&["sh", "-c", "printf '%s' \"$1\" | cliphist delete", "sh", line]);
        }
        self.all.borrow_mut().remove(i);
        let n = self.picked.get();
        self.filter();
        self.set_picked(n.min(self.hits.borrow().len().saturating_sub(1)));
    }
}
