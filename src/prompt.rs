//! The kit's dialog: a question put to the user in ostrov's look, the polkit agent's (a password to do something
//! as root), ssh's askpass (a key's passphrase, or a yes to use a key), a script's (`ostrov dialog JSON`), a
//! plugin's. The screen dimmed, a surface in its middle with an icon, a title, what is asked, then what it asks
//! for: a yes or no alone, a line of text, a password, one of some options, a form (settings/'s, its answers
//! written nowhere), and what went wrong last time. A plugin's says whose it is above it all, so none passes for
//! polkit or ostrov. The keyboard is all its own while it is up, and works it all: Tab and Shift+Tab around, the
//! focused one ringed, Enter or Space its own, Escape a no. One question at a time: the next waits for the last's
//! answer.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use futures_util::future::{select, Either};
use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;

use crate::i18n::{fill, t};
use crate::style::{clear, label};

/// What a question asks for, and so what its answer is.
pub enum Kind {
    /// a yes ("") or no
    Confirm,
    /// a line of text, its placeholder and what it starts as
    Text { placeholder: String, value: String },
    /// a password
    Secret,
    /// one of these, the one picked
    Choice(Vec<String>),
    /// a settings form, its schema as JSON (read already: an Ask goes between threads, a Schema does not), its
    /// answers as a JSON object
    Form(Value),
}

/// A question.
pub struct Ask {
    pub icon: String,
    pub title: String,
    pub text: String,
    pub kind: Kind,
    /// what went wrong last time (a wrong password), said in red
    pub error: String,
    /// its buttons' words
    pub ok: String,
    pub cancel: String,
    /// who asks, its icon and name ("" ostrov itself)
    pub from: (String, String),
    /// the answer, None for a no
    pub reply: async_channel::Sender<Option<String>>,
}

impl Ask {
    pub fn new(icon: &str, title: &str, text: &str, kind: Kind, reply: async_channel::Sender<Option<String>>) -> Ask {
        Ask {
            icon: icon.into(),
            title: title.into(),
            text: text.into(),
            kind,
            error: String::new(),
            ok: t("OK").into(),
            cancel: t("Cancel").into(),
            from: Default::default(),
            reply,
        }
    }

    /// A question as JSON says it (docs/plugins.md, Dialogs): {kind, icon, title, text, error, ok, cancel,
    /// placeholder, value, options, schema}, every field but kind optional.
    pub fn from_json(v: &Value, reply: async_channel::Sender<Option<String>>) -> Result<Ask, String> {
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let kind = match v["kind"].as_str().unwrap_or("") {
            "confirm" => Kind::Confirm,
            "text" => Kind::Text { placeholder: s("placeholder"), value: s("value") },
            "secret" => Kind::Secret,
            "choice" => {
                let o: Vec<String> =
                    serde_json::from_value(v["options"].clone()).map_err(|e| format!("options: {e}"))?;
                if o.is_empty() {
                    return Err("a choice with no options".into());
                }
                Kind::Choice(o)
            }
            "form" => {
                let read = serde_json::from_value::<crate::settings::Schema>(v["schema"].clone());
                read.map_err(|e| format!("schema: {e}"))?;
                Kind::Form(v["schema"].clone())
            }
            k => return Err(format!("no dialog kind {k:?}: confirm, text, secret, choice, form")),
        };
        let mut a = Ask::new(&s("icon"), &s("title"), &s("text"), kind, reply);
        a.error = s("error");
        for (field, key) in [(&mut a.ok, "ok"), (&mut a.cancel, "cancel")] {
            if let Some(w) = v[key].as_str().filter(|w| !w.is_empty()) {
                *field = w.into();
            }
        }
        Ok(a)
    }
}

pub struct Prompts {
    win: gtk4::ApplicationWindow,
    from: gtk4::Box,
    from_icon: gtk4::Image,
    from_name: gtk4::Label,
    icon: gtk4::Image,
    title: gtk4::Label,
    text: gtk4::Label,
    entry: gtk4::PasswordEntry,
    line: gtk4::Entry,
    body: gtk4::Box,
    ok: gtk4::Button,
    cancel: gtk4::Button,
    error: gtk4::Label,
    queue: RefCell<VecDeque<Ask>>,
    current: RefCell<Option<async_channel::Sender<Option<String>>>>,
    /// the answer of the question up, should it be a yes
    answer: RefCell<Box<dyn Fn() -> String>>,
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
        let from = gtk4::Box::new(Orientation::Horizontal, 6);
        let from_icon = gtk4::Image::new();
        let from_name = label("", "dim");
        from.append(&from_icon);
        from.append(&from_name);
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
        let line = gtk4::Entry::new();
        let body = gtk4::Box::new(Orientation::Vertical, 2);
        let error = label("", "error");
        error.set_wrap(true);
        let buttons = gtk4::Box::new(Orientation::Horizontal, 8);
        buttons.set_halign(Align::End);
        let cancel = gtk4::Button::with_label(t("Cancel"));
        cancel.add_css_class("chip");
        let ok = gtk4::Button::with_label(t("OK"));
        ok.add_css_class("connect");
        buttons.append(&cancel);
        buttons.append(&ok);
        let parts = [
            from.upcast_ref::<gtk4::Widget>(),
            head.upcast_ref(),
            text.upcast_ref(),
            entry.upcast_ref(),
            line.upcast_ref(),
            body.upcast_ref(),
            error.upcast_ref(),
            buttons.upcast_ref(),
        ];
        for w in parts {
            card.append(w);
        }
        win.set_child(Some(&card));

        let p = Rc::new(Prompts {
            win: win.clone(),
            from,
            from_icon,
            from_name,
            icon,
            title,
            text,
            entry: entry.clone(),
            line: line.clone(),
            body,
            ok: ok.clone(),
            cancel: cancel.clone(),
            error,
            queue: RefCell::default(),
            current: RefCell::default(),
            answer: RefCell::new(Box::new(String::new)),
        });
        let answer = {
            let p = Rc::downgrade(&p);
            move |yes: bool| {
                if let Some(p) = p.upgrade() {
                    let a = yes.then(|| (p.answer.borrow())());
                    p.finish(a);
                }
            }
        };
        let a = answer.clone();
        ok.connect_clicked(move |_| a(true));
        let a = answer.clone();
        entry.connect_activate(move |_| a(true));
        let a = answer.clone();
        line.connect_activate(move |_| a(true));
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
        self.finish(None);
    }

    /// The question answering to reply taken back, answered no, whether it is up or still waiting.
    pub fn withdraw(self: &Rc<Self>, reply: &async_channel::Sender<Option<String>>) {
        if self.current.borrow().as_ref().is_some_and(|c| c.same_channel(reply)) {
            return self.finish(None);
        }
        self.queue.borrow_mut().retain(|a| !a.reply.same_channel(reply));
        let _ = reply.try_send(None);
    }

    /// The question up answered, the next one up.
    fn finish(&self, a: Option<String>) {
        if let Some(reply) = self.current.take() {
            let _ = reply.send_blocking(a);
        }
        self.entry.set_text("");
        self.line.set_text("");
        clear(&self.body);
        self.next();
    }

    /// The next question up, or the window gone with none left.
    fn next(&self) {
        let Some(a) = self.queue.borrow_mut().pop_front() else {
            self.win.set_visible(false);
            return;
        };
        self.from.set_visible(!a.from.1.is_empty());
        self.from_icon.set_icon_name(Some(&a.from.0));
        self.from_name.set_text(&fill(t("{} asks"), &[&a.from.1]));
        self.icon.set_icon_name(Some(if a.icon.is_empty() { "dialog-password-symbolic" } else { &a.icon }));
        self.title.set_text(&a.title);
        self.text.set_text(&a.text);
        self.text.set_visible(!a.text.is_empty());
        self.error.set_text(&a.error);
        self.error.set_visible(!a.error.is_empty());
        self.ok.set_label(&a.ok);
        self.cancel.set_label(&a.cancel);
        self.entry.set_visible(matches!(a.kind, Kind::Secret));
        self.line.set_visible(matches!(a.kind, Kind::Text { .. }));
        self.body.set_visible(matches!(a.kind, Kind::Choice(_) | Kind::Form(_)));
        self.ok.set_visible(!matches!(a.kind, Kind::Choice(_)));
        let answer: Box<dyn Fn() -> String> = match &a.kind {
            Kind::Confirm => Box::new(String::new),
            Kind::Secret => {
                let e = self.entry.clone();
                Box::new(move || e.text().to_string())
            }
            Kind::Text { placeholder, value } => {
                self.line.set_placeholder_text(Some(placeholder));
                self.line.set_text(value);
                let e = self.line.clone();
                Box::new(move || e.text().to_string())
            }
            // an option's row picks it and says yes
            Kind::Choice(options) => {
                let picked: Rc<RefCell<String>> = Rc::default();
                for o in options {
                    let (o2, p, ok) = (o.clone(), picked.clone(), self.ok.clone());
                    self.body.append(&crate::ui::row("", o, "", false, move || {
                        *p.borrow_mut() = o2.clone();
                        ok.emit_clicked();
                    }));
                }
                Box::new(move || picked.borrow().clone())
            }
            Kind::Form(schema) => {
                let schema = serde_json::from_value(schema.clone()).unwrap_or_default();
                let (form, answers) = crate::settings::form::answers(&schema);
                self.body.append(&form);
                Box::new(move || Value::Object(answers.borrow().clone()).to_string())
            }
        };
        *self.answer.borrow_mut() = answer;
        *self.current.borrow_mut() = Some(a.reply);
        self.win.set_visible(true);
        // the focus ringed from the start, not only after a first key
        self.win.set_focus_visible(true);
        // the focus where the answer is given
        let _ = match a.kind {
            Kind::Secret => self.entry.grab_focus(),
            Kind::Text { .. } => self.line.grab_focus(),
            Kind::Choice(_) | Kind::Form(_) => self.body.child_focus(gtk4::DirectionType::TabForward),
            Kind::Confirm => self.ok.grab_focus(),
        };
    }
}

/// A question asked as JSON says it, from someone ((icon, name), a plugin), taken back as a no after limit: its
/// answer, None for a no.
pub async fn dialog(
    spec: &Value,
    from: Option<(String, String)>,
    limit: Option<Duration>,
) -> Result<Option<String>, String> {
    let prompts = crate::prompts().ok_or("ostrov is starting")?;
    let (reply, answer) = async_channel::bounded(1);
    let mut a = Ask::from_json(spec, reply.clone())?;
    if let Some(f) = from {
        a.from = f;
    }
    prompts.ask(a);
    let Some(limit) = limit else { return Ok(answer.recv().await.ok().flatten()) };
    match select(std::pin::pin!(answer.recv()), std::pin::pin!(glib::timeout_future(limit))).await {
        Either::Left((r, _)) => Ok(r.ok().flatten()),
        Either::Right(_) => {
            prompts.withdraw(&reply);
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dialogs_read() {
        let (tx, _rx) = async_channel::bounded(1);
        let text = json!({"kind": "text", "title": "Rename", "value": "Hello", "ok": "Rename"});
        let a = Ask::from_json(&text, tx.clone()).unwrap();
        assert!(matches!(&a.kind, Kind::Text { value, .. } if value == "Hello"));
        assert_eq!((a.title.as_str(), a.ok.as_str(), a.cancel.as_str()), ("Rename", "Rename", t("Cancel")));
        let choice = Ask::from_json(&json!({"kind": "choice", "options": ["a", "b"]}), tx.clone()).unwrap();
        assert!(matches!(choice.kind, Kind::Choice(o) if o.len() == 2));
        assert!(Ask::from_json(&json!({"kind": "choice", "options": []}), tx.clone()).is_err());
        assert!(Ask::from_json(&json!({"kind": "wizard"}), tx.clone()).is_err());
        let form = json!({"kind": "form", "schema": {"sections": [{"title": "T", "fields": [
            {"key": "token", "title": "Token", "type": "secret"}]}]}});
        assert!(matches!(Ask::from_json(&form, tx.clone()).unwrap().kind, Kind::Form(_)));
        assert!(Ask::from_json(&json!({"kind": "form", "schema": {"fields": []}}), tx).is_err());
    }
}
