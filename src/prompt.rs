//! A question put to the user, in ostrov's look, for the polkit agent (a password to do something as root) and
//! ssh's askpass (a key's passphrase, or a yes to use a key): the screen dimmed, a surface in its middle with an
//! icon, a title, what is asked, the password (or nothing, a yes or no alone), and what went wrong last time. The
//! keyboard is all its own while it is up, and works it all: Tab and Shift+Tab between the password, Cancel and
//! OK, the focused one ringed, Enter or Space its own, Escape a no. One question at a time: the next waits for the
//! last's answer.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::style::label;

/// A question.
pub struct Ask {
    pub icon: String,
    pub title: String,
    pub text: String,
    /// asked for a password (hidden), else for a yes or no alone
    pub secret: bool,
    /// what went wrong last time (a wrong password), said in red
    pub error: String,
    /// the answer: the password ("" for a yes), None for a no
    pub reply: async_channel::Sender<Option<String>>,
}

pub struct Prompts {
    win: gtk4::ApplicationWindow,
    icon: gtk4::Image,
    title: gtk4::Label,
    text: gtk4::Label,
    entry: gtk4::PasswordEntry,
    ok: gtk4::Button,
    error: gtk4::Label,
    queue: RefCell<VecDeque<Ask>>,
    current: RefCell<Option<async_channel::Sender<Option<String>>>>,
}

impl Prompts {
    pub fn new(app: &gtk4::Application) -> Rc<Prompts> {
        let win = gtk4::ApplicationWindow::new(app);
        win.init_layer_shell();
        win.set_layer(Layer::Overlay);
        win.set_namespace(Some("ostrov-prompt"));
        for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            win.set_anchor(e, true);
        }
        win.set_exclusive_zone(-1);
        win.set_keyboard_mode(KeyboardMode::Exclusive);
        win.add_css_class("prompt");

        let card = gtk4::Box::new(Orientation::Vertical, 10);
        card.add_css_class("surface");
        card.set_halign(Align::Center);
        card.set_valign(Align::Center);
        card.set_size_request(380, -1);
        let head = gtk4::Box::new(Orientation::Horizontal, 12);
        let icon = gtk4::Image::new();
        icon.set_pixel_size(32);
        let title = label("", "title");
        title.set_wrap(true);
        head.append(&icon);
        head.append(&title);
        let text = label("", "dim");
        text.set_wrap(true);
        text.set_max_width_chars(48);
        let entry = gtk4::PasswordEntry::new();
        entry.set_show_peek_icon(true);
        let error = label("", "error");
        error.set_wrap(true);
        let buttons = gtk4::Box::new(Orientation::Horizontal, 8);
        buttons.set_halign(Align::End);
        let cancel = gtk4::Button::with_label("Cancel");
        cancel.add_css_class("chip");
        let ok = gtk4::Button::with_label("OK");
        ok.add_css_class("connect");
        buttons.append(&cancel);
        buttons.append(&ok);
        for w in [head.upcast_ref::<gtk4::Widget>(), text.upcast_ref(), entry.upcast_ref(), error.upcast_ref(), buttons.upcast_ref()] {
            card.append(w);
        }
        win.set_child(Some(&card));

        let p = Rc::new(Prompts {
            win: win.clone(),
            icon,
            title,
            text,
            entry: entry.clone(),
            ok: ok.clone(),
            error,
            queue: RefCell::default(),
            current: RefCell::default(),
        });
        let answer = {
            let p = Rc::downgrade(&p);
            move |yes: bool| {
                if let Some(p) = p.upgrade() {
                    let a = yes.then(|| p.entry.text().to_string());
                    p.entry.set_text("");
                    if let Some(reply) = p.current.take() {
                        let _ = reply.send_blocking(a);
                    }
                    p.next();
                }
            }
        };
        let a = answer.clone();
        ok.connect_clicked(move |_| a(true));
        let a = answer.clone();
        entry.connect_activate(move |_| a(true));
        let a = answer.clone();
        cancel.connect_clicked(move |_| a(false));
        let keys = gtk4::EventControllerKey::new();
        keys.connect_key_pressed(move |_, k, _, _| {
            // Escape says no wherever the focus is; Enter and Space are the focused widget's (the password's
            // Enter is OK, a button's its own), so Tab to Cancel and Enter says no
            if k != gtk4::gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            answer(false);
            glib::Propagation::Stop
        });
        win.add_controller(keys);
        p
    }

    /// The question asked now, or once those before it are answered.
    pub fn ask(self: &Rc<Self>, a: Ask) {
        self.queue.borrow_mut().push_back(a);
        if self.current.borrow().is_none() {
            self.next();
        }
    }

    /// The question asked now taken back (polkit's CancelAuthentication), answered no.
    pub fn cancel(self: &Rc<Self>) {
        if let Some(reply) = self.current.take() {
            let _ = reply.send_blocking(None);
        }
        self.entry.set_text("");
        self.next();
    }

    /// The next question up, or the window gone with none left.
    fn next(&self) {
        let Some(a) = self.queue.borrow_mut().pop_front() else {
            self.win.set_visible(false);
            return;
        };
        self.icon.set_icon_name(Some(if a.icon.is_empty() { "dialog-password-symbolic" } else { &a.icon }));
        self.title.set_text(&a.title);
        self.text.set_text(&a.text);
        self.text.set_visible(!a.text.is_empty());
        self.entry.set_visible(a.secret);
        self.error.set_text(&a.error);
        self.error.set_visible(!a.error.is_empty());
        *self.current.borrow_mut() = Some(a.reply);
        self.win.set_visible(true);
        // the focus ringed from the start, not only after a first key
        self.win.set_focus_visible(true);
        // the focus where the answer is given: the password, or OK for a yes or no
        if a.secret {
            self.entry.grab_focus();
        } else {
            self.ok.grab_focus();
        }
    }
}
