//! The first run's welcome: a card in the middle of the screen walking through what to settle once. The language
//! and the look (cc/appearance.rs's themes and accents, the language and the density), what ostrov puts into
//! Hyprland by itself (its rules, its keys and the user's binds it keeps, [hyprland] turning either off), the
//! battery's charge limit where there is one (the udev rule installed through pkexec where the thresholds are
//! root's), then a few tips. Shown once, at the first start of an ostrov with no config yet (one with a config
//! has been run before, its user not bothered); finished or skipped, ~/.local/state/ostrov/welcomed says so.
//! `ostrov welcome` shows it again.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;

use crate::i18n::{fill, t};
use crate::style::{clear, label};
use crate::ui::setting;

fn marker() -> PathBuf {
    crate::hub::home().join(".local/state/ostrov/welcomed")
}

/// Whether the welcome is due: not shown before, and no config (an ostrov run before this one had the welcome,
/// marked shown at once).
fn due(marker: &Path, config: &Path) -> bool {
    if marker.exists() {
        return false;
    }
    if config.exists() {
        mark(marker);
        return false;
    }
    true
}

fn mark(marker: &Path) {
    let r = marker.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(marker, ""));
    if let Err(e) = r {
        eprintln!("ostrov: {}: {e}", marker.display());
    }
}

pub struct Welcome {
    win: gtk4::ApplicationWindow,
    stack: gtk4::Stack,
    dots: gtk4::Box,
    back: gtk4::Button,
    next: gtk4::Button,
    /// the steps' pages after the look's, made anew at every open (the binds, the battery as they are then)
    rest: RefCell<Vec<gtk4::Widget>>,
    step: Cell<usize>,
    /// the language as it was at the open: another at Done starts ostrov again in it
    lang: RefCell<String>,
}

impl Welcome {
    pub fn new(app: &gtk4::Application) -> Rc<Welcome> {
        let win = gtk4::ApplicationWindow::new(app);
        win.init_layer_shell();
        win.set_layer(Layer::Overlay);
        win.set_namespace(Some("ostrov-welcome"));
        for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            win.set_anchor(e, true);
        }
        win.set_exclusive_zone(-1);
        win.set_keyboard_mode(KeyboardMode::OnDemand);
        win.add_css_class("prompt");

        let card = gtk4::Box::new(Orientation::Vertical, 12);
        card.add_css_class("surface");
        card.add_css_class("welcome");
        card.set_halign(Align::Center);
        card.set_valign(Align::Center);
        card.set_size_request(560, -1);
        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);
        stack.set_vhomogeneous(false);
        stack.set_interpolate_size(true);
        stack.add_named(&look(), Some("0"));
        // as wide as the card, not as the themes' and the accents' rows would have it: those wrap
        let fit = gtk4::ScrolledWindow::new();
        fit.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Never);
        fit.set_propagate_natural_height(true);
        fit.set_child(Some(&stack));
        card.append(&fit);

        let foot = gtk4::Box::new(Orientation::Horizontal, 8);
        let skip = crate::ui::chip(t("Skip"));
        let dots = gtk4::Box::new(Orientation::Horizontal, 6);
        dots.set_hexpand(true);
        dots.set_halign(Align::Center);
        dots.set_valign(Align::Center);
        let back = crate::ui::chip(t("Back"));
        let next = crate::ui::primary(t("Next"));
        for w in [skip.upcast_ref::<gtk4::Widget>(), dots.upcast_ref(), back.upcast_ref(), next.upcast_ref()] {
            foot.append(w);
        }
        card.append(&foot);
        win.set_child(Some(&card));

        let w = Rc::new(Welcome {
            win: win.clone(),
            stack,
            dots,
            back: back.clone(),
            next: next.clone(),
            rest: RefCell::default(),
            step: Cell::new(0),
            lang: RefCell::default(),
        });
        let me = Rc::downgrade(&w);
        skip.connect_clicked(move |_| {
            if let Some(w) = me.upgrade() {
                w.close(false)
            }
        });
        let me = Rc::downgrade(&w);
        back.connect_clicked(move |_| {
            if let Some(w) = me.upgrade() {
                w.go(w.step.get().saturating_sub(1))
            }
        });
        let me = Rc::downgrade(&w);
        next.connect_clicked(move |_| {
            if let Some(w) = me.upgrade() {
                if w.step.get() + 1 < w.count() { w.go(w.step.get() + 1) } else { w.close(true) }
            }
        });
        let keys = gtk4::EventControllerKey::new();
        let me = Rc::downgrade(&w);
        keys.connect_key_pressed(move |_, k, _, _| {
            if k != gtk4::gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            if let Some(w) = me.upgrade() {
                w.close(false);
            }
            glib::Propagation::Stop
        });
        win.add_controller(keys);
        w
    }

    /// Shown at the start when it is due, once ostrov's layer rules are in Hyprland (a layer takes its rules as it
    /// is mapped).
    pub fn first_run(self: &Rc<Self>) {
        if due(&marker(), &crate::config::path()) {
            let me = self.clone();
            // ponytail: a second for the rules' batch, not a signal of it being done
            glib::timeout_add_local_once(std::time::Duration::from_secs(1), move || me.open());
        }
    }

    /// Shown from its first step, the steps after the look's made as things are now.
    pub fn open(self: &Rc<Self>) {
        for p in self.rest.take() {
            self.stack.remove(&p);
        }
        let mut rest = vec![hyprland()];
        if !crate::modules::battery::limit::state().is_null() {
            rest.push(battery());
        }
        rest.push(done());
        for (i, p) in rest.iter().enumerate() {
            self.stack.add_named(p, Some(&(i + 1).to_string()));
        }
        *self.rest.borrow_mut() = rest;
        *self.lang.borrow_mut() = crate::config::load().appearance.language;
        clear(&self.dots);
        for _ in 0..self.count() {
            let d = gtk4::Box::new(Orientation::Horizontal, 0);
            d.add_css_class("dot");
            self.dots.append(&d);
        }
        self.go(0);
        self.win.set_visible(true);
        let _ = self.next.grab_focus();
    }

    fn count(&self) -> usize {
        1 + self.rest.borrow().len()
    }

    fn go(&self, step: usize) {
        self.step.set(step);
        self.stack.set_visible_child_name(&step.to_string());
        let mut dot = self.dots.first_child();
        let mut i = 0;
        while let Some(d) = dot {
            if i == step { d.add_css_class("focused") } else { d.remove_css_class("focused") }
            dot = d.next_sibling();
            i += 1;
        }
        self.back.set_sensitive(step > 0);
        self.next.set_label(t(if step + 1 == self.count() { "Done" } else { "Next" }));
    }

    /// Gone, marked shown; Done in another language than at the open starts ostrov again in it.
    fn close(&self, done: bool) {
        self.win.set_visible(false);
        mark(&marker());
        if done && crate::config::load().appearance.language != *self.lang.borrow() {
            crate::restart();
        }
    }
}

/// A step's page: its title, what it is about, then its own.
fn page(title: &str, about: &str) -> gtk4::Box {
    let p = gtk4::Box::new(Orientation::Vertical, 8);
    p.append(&label(title, "page-title"));
    if !about.is_empty() {
        let a = label(about, "dim");
        a.set_wrap(true);
        a.set_max_width_chars(60);
        p.append(&a);
    }
    p
}

/// The language and the look: the Appearance page's themes and accents, its language and density.
fn look() -> gtk4::Widget {
    let p = page(t("Welcome to ostrov"), t("Pick a look to start with; the control centre's Appearance has the rest."));
    let body = crate::cc::appearance::body(&["language", "density"]);
    body.remove_css_class("page-body");
    p.append(&body);
    p.upcast()
}

/// Another polkit agent there first (an autostart meant for other desktops): its dialogs asking the passwords in
/// its own look, and a way to keep it out of Hyprland.
fn polkit(p: &gtk4::Box) {
    let crate::polkit::Answering::Other(name) = crate::polkit::answering() else { return };
    let name = if name.is_empty() { t("another agent").to_string() } else { name };
    let note = label(&fill(t("Passwords are asked by {} now, in its own look, not ostrov's."), &[&name]), "field-help");
    note.set_wrap(true);
    note.set_xalign(0.0);
    p.append(&note);
    let Some(system) = crate::polkit::autostart(&name) else { return };
    let b = crate::ui::chip(t("Keep it out of Hyprland"));
    b.set_halign(gtk4::Align::Start);
    b.connect_clicked(move |b| {
        let said = match crate::polkit::keep_out(&system) {
            Ok(_) => t("Done: ostrov asks from the next login."),
            Err(e) => {
                eprintln!("ostrov: polkit: {e}");
                t("Not done: ~/.config/autostart is not writable.")
            }
        };
        b.set_label(said);
        b.set_sensitive(false);
    });
    p.append(&b);
}

/// Of Hyprland: what ostrov puts into it, and [hyprland]'s two switches.
fn hyprland() -> gtk4::Widget {
    if crate::wm::wm() != crate::wm::Wm::Hyprland {
        let not = t("Not under Hyprland: what ostrov sets up there by itself is Hyprland's alone.");
        return page("Hyprland", not).upcast();
    }
    let p = page("Hyprland", t("ostrov sets these up in the running Hyprland at every start and config reload; \
        your hyprland.conf is left as it is."));
    let cfg = crate::config::load().hyprland;
    let switch = |key: &'static str, on: bool, title: &str, help: &str| {
        let s = gtk4::Switch::new();
        s.set_active(on);
        s.connect_state_set(move |_, on| {
            if let Err(e) = crate::settings::write("hyprland", key, Some(&Value::Bool(on)), false) {
                eprintln!("ostrov: hyprland: {e}");
            }
            glib::Propagation::Proceed
        });
        p.append(&setting(title, help, &s, false));
    };
    switch("rules", cfg.rules, t("Layer rules"), t("Blur under the bar, the panels and the dialogs."));
    switch("binds", cfg.binds, t("Keys"), t("Its keys below, each only where it is free: a bind of yours is kept. \
        Turned off, from the next start."));
    let lock = label(t("And always: misc:allow_session_lock_restore, so a new ostrov takes the lock over from one \
        that died locked."), "field-help");
    lock.set_wrap(true);
    lock.set_xalign(0.0);
    p.append(&lock);
    polkit(&p);

    let grid = gtk4::Grid::new();
    grid.set_column_spacing(12);
    grid.set_row_spacing(4);
    let binds: Vec<Value> = serde_json::from_str(&crate::wm::hyprctl("j/binds")).unwrap_or_default();
    for (i, k) in crate::modules::hyprland::keys(&binds).into_iter().enumerate() {
        let theirs = k.by.as_deref().filter(|b| !(b.contains("ostrov") && b.ends_with(&format!(" {}", k.cmd))));
        let (col, row) = (2 * (i as i32 % 2), i as i32 / 2);
        let at = label(&pretty(&k.at), "");
        let what = match theirs {
            Some(b) => label(&fill(t("kept yours: {}"), &[&b]), "dim"),
            None => label(&k.cmd, "dim"),
        };
        what.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        what.set_max_width_chars(10);
        what.set_hexpand(true);
        for (w, c) in [(&at, col), (&what, col + 1)] {
            w.set_xalign(0.0);
            grid.attach(w, c, row, 1, 1);
        }
    }
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk4::PolicyType::Never);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(220);
    scroll.set_child(Some(&grid));
    p.append(&scroll);
    p.upcast()
}

/// "SUPER SHIFT, S" as a key is said: Super+Shift+S.
fn pretty(at: &str) -> String {
    let (mods, key) = at.split_once(',').unwrap_or(("", at));
    let mut words: Vec<String> = mods
        .split_whitespace()
        .map(|m| m[..1].to_uppercase() + &m[1..].to_lowercase())
        .collect();
    let key = key.trim();
    words.push(key.strip_prefix("XF86").unwrap_or(key).to_string());
    words.join("+")
}

/// The charge limit: picked where the thresholds are the user's, else the udev rule letting them installed.
fn battery() -> gtk4::Widget {
    let p = page(t("Battery"), t("A laptop that lives on its charger ages its battery slower charged short of \
        full."));
    let body = gtk4::Box::new(Orientation::Vertical, 6);
    battery_fill(&body);
    p.append(&body);
    p.upcast()
}

fn battery_fill(body: &gtk4::Box) {
    clear(body);
    let st = crate::modules::battery::limit::state();
    let error = label("", "error");
    error.set_wrap(true);
    error.set_xalign(0.0);
    error.set_visible(false);
    let said = {
        let error = error.clone();
        move |r: Result<(), String>| {
            if let Err(e) = r {
                error.set_text(&e);
                error.set_visible(true);
            }
        }
    };
    if st["writable"] == true {
        const LIMITS: [u64; 3] = [80, 90, 100];
        let names = ["80%", "90%", "100%"];
        let on: Vec<bool> = LIMITS.iter().map(|l| st["end"].as_u64() == Some(*l)).collect();
        let chips = crate::ui::options(&names, &on, false, move |i, _| {
            let said = said.clone();
            crate::hub::service_then(
                vec!["battery".into(), "limit".into(), LIMITS[i].to_string()],
                None,
                said,
            );
        });
        body.append(&setting(t("Charge limit"), t("Charging stops here, and starts again 5% below it."), &chips, true));
    } else {
        let install = crate::ui::primary(t("Install"));
        let b = body.clone();
        install.connect_clicked(move |button| {
            button.set_sensitive(false);
            let (b, said) = (b.clone(), said.clone());
            crate::hub::service_then(vec!["battery".into(), "limit".into(), "install".into()], None, move |r| {
                match r {
                    Ok(()) => battery_fill(&b),
                    Err(e) => said(Err(fill(t("Not installed: {}"), &[&e]))),
                }
            });
        });
        let help = t("The thresholds are root's until a udev rule gives them to you; it is installed as root, your \
            password asked.");
        body.append(&setting(t("Charge limit"), help, &install, false));
    }
    body.append(&error);
}

/// The last step: where things are.
fn done() -> gtk4::Widget {
    let p = page(t("All set"), t("A few things to know:"));
    let run = pretty(&crate::modules::hyprland::combo("run", "SUPER, D"));
    let tips = [
        ("system-search-symbolic", fill(t("{}: the launcher, apps and more"), &[&run])),
        ("document-edit-symbolic", t("A panel's Edit: its widgets added, moved and sized").to_string()),
        ("preferences-system-symbolic", t("ostrov settings: every setting").to_string()),
        ("dialog-information-symbolic", t("ostrov doctor: what ostrov finds missing").to_string()),
        ("go-home-symbolic", t("ostrov welcome: this again").to_string()),
    ];
    for (icon, text) in tips {
        let row = gtk4::Box::new(Orientation::Horizontal, 10);
        row.append(&gtk4::Image::from_icon_name(icon));
        let l = label(&text, "");
        l.set_wrap(true);
        l.set_xalign(0.0);
        row.append(&l);
        p.append(&row);
    }
    p.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_once_and_never_with_a_config() {
        let dir = std::env::temp_dir().join(format!("ostrov-welcome-{}", std::process::id()));
        let (marker, config) = (dir.join("state/welcomed"), dir.join("config.toml"));
        assert!(due(&marker, &config));
        mark(&marker);
        assert!(!due(&marker, &config));
        std::fs::remove_file(&marker).unwrap();
        std::fs::write(&config, "").unwrap();
        assert!(!due(&marker, &config));
        assert!(marker.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keys_said_plainly() {
        assert_eq!(pretty("SUPER SHIFT, S"), "Super+Shift+S");
        assert_eq!(pretty(", XF86AudioMute"), "AudioMute");
        assert_eq!(pretty("SUPER, D"), "Super+D");
    }
}
