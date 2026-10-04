//! A schema drawn as a form (ui.rs's kit: settings, switches, chips, sliders, entries), each field written back
//! as it is set (a switch, a chip, a slider at rest) or typed (Enter, or the focus leaving), checked first, what is
//! wrong said under it; and the control centre's Settings page: the entries, a form each.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use serde_json::{Map, Value};

use super::{check, entries, fmt_duration, off_thread, value, write, Cond, Field, Kind, Schema, Section};
use crate::style::{clear, label};
use crate::ui::{header, options, row, setting};

/// A field's value handed to be checked and written; whether it was.
type Put = Rc<dyn Fn(Value) -> bool>;

/// The answers of a form written nowhere (a dialog's), secrets among them.
pub type Answers = Rc<RefCell<Map<String, Value>>>;

/// A section's form: its values as last written, the fields shown only while another has some value; a dialog's
/// its answers instead of the config.
struct State {
    table: String,
    vals: RefCell<Map<String, Value>>,
    shown: RefCell<Vec<(Cond, gtk4::Widget)>>,
    answers: Option<Answers>,
}

impl State {
    fn put(&self, f: &Field, v: Value) -> Result<(), String> {
        let v = check(&f.kind, v)?;
        match &self.answers {
            Some(a) => drop(a.borrow_mut().insert(f.key.clone(), v.clone())),
            None => write(&self.table, &f.key, Some(&v), f.integer())?,
        }
        self.vals.borrow_mut().insert(f.key.clone(), v);
        self.fit();
        Ok(())
    }

    fn fit(&self) {
        let vals = self.vals.borrow();
        for (c, w) in self.shown.borrow().iter() {
            w.set_visible(vals.get(&c.key) == Some(&c.equals));
        }
    }
}

/// A schema's form written nowhere, for a dialog: its fields from their defaults, a secret a plain password, what
/// is set gathered in the answers (every section's keys in one object), no buttons.
pub fn answers(schema: &Schema) -> (gtk4::Box, Answers) {
    let answers = Answers::default();
    let bx = gtk4::Box::new(Orientation::Vertical, 0);
    for s in &schema.sections {
        bx.append(&fill(s, Some(answers.clone())));
    }
    (bx, answers)
}

/// A schema's form, a heading a section when it has more than one.
pub fn form(schema: &Schema) -> gtk4::Box {
    let bx = gtk4::Box::new(Orientation::Vertical, 0);
    for s in &schema.sections {
        if schema.sections.len() > 1 {
            let t = label(&s.title, "title");
            t.add_css_class("form-section");
            bx.append(&t);
        }
        bx.append(&section(s));
    }
    bx
}

/// A section's fields, as the config has them now.
pub fn section(sec: &Section) -> gtk4::Box {
    fill(sec, None)
}

fn fill(sec: &Section, answers: Option<Answers>) -> gtk4::Box {
    let bx = gtk4::Box::new(Orientation::Vertical, 0);
    if !sec.help.is_empty() {
        let h = label(&sec.help, "field-help");
        h.set_wrap(true);
        bx.append(&h);
    }
    let st = Rc::new(State { table: sec.key.clone(), vals: RefCell::default(), shown: RefCell::default(), answers });
    for f in &sec.fields {
        let cur = if st.answers.is_some() { f.default.clone() } else { value(&sec.key, f) };
        st.vals.borrow_mut().insert(f.key.clone(), cur.clone());
        if let Some(a) = st.answers.as_ref().filter(|_| !cur.is_null()) {
            a.borrow_mut().insert(f.key.clone(), cur.clone());
        }
        let err = label("", "error");
        err.set_wrap(true);
        err.set_visible(false);
        let put: Put = {
            let (st, f, err) = (st.clone(), f.clone(), err.clone());
            Rc::new(move |v| {
                let r = st.put(&f, v);
                err.set_text(r.as_ref().err().map_or("", String::as_str));
                err.set_visible(r.is_err());
                r.is_ok()
            })
        };
        let (ctl, wide) = match f.kind {
            Kind::Secret if st.answers.is_some() => {
                let e = gtk4::PasswordEntry::new();
                e.set_show_peek_icon(true);
                e.set_hexpand(true);
                e.connect_changed(move |e| {
                    put(e.text().as_str().into());
                });
                (e.upcast(), true)
            }
            _ => control(f, &cur, &format!("{}.{}", sec.key, f.key), put, &err),
        };
        let r = setting(&f.title, &f.help, &ctl, wide);
        r.append(&err);
        if !f.actions.is_empty() && st.answers.is_none() {
            r.append(&actions(f, &st));
        }
        if let Some(c) = &f.visible_if {
            st.shown.borrow_mut().push((c.clone(), r.clone().upcast()));
        }
        bx.append(&r);
    }
    st.fit();
    bx
}

/// A field's buttons, what the last one pressed came to beside them.
fn actions(f: &Field, st: &Rc<State>) -> gtk4::Box {
    let line = gtk4::Box::new(Orientation::Horizontal, 6);
    let said = label("", "dim");
    said.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    for a in &f.actions {
        let b = gtk4::Button::with_label(&a.label);
        b.add_css_class("chip");
        // named by its id (a plugin's host finds it so); without a run (a plugin's not wired yet) inert
        b.set_widget_name(&a.id);
        b.set_sensitive(a.run.is_some());
        let (run, st, key, said) = (a.run.clone(), st.clone(), f.key.clone(), said.clone());
        b.connect_clicked(move |b| {
            let Some(run) = &run else { return };
            said.set_text("…");
            said.remove_css_class("error");
            b.set_sensitive(false);
            let (said, b) = (said.clone(), b.clone());
            let v = st.vals.borrow().get(&key).cloned().unwrap_or_default();
            run(&v, Rc::new(move |r: Result<String, String>| {
                b.set_sensitive(true);
                match r {
                    Ok(t) => said.set_text(&t),
                    Err(e) => {
                        said.set_text(&e);
                        said.add_css_class("error");
                    }
                }
            }));
        });
        line.append(&b);
    }
    line.append(&said);
    line
}

/// put once the value has rested a moment: a slider dragged writes the file once.
fn settled(put: Put) -> Rc<dyn Fn(Value)> {
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    Rc::new(move |v| {
        if let Some(id) = pending.borrow_mut().take() {
            id.remove();
        }
        let (put, p) = (put.clone(), pending.clone());
        *pending.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_millis(250), move || {
            p.borrow_mut().take();
            put(v);
        }));
    })
}

/// An entry written on Enter or as the focus leaves it, when its text changed.
fn entry(text: &str, put: Put) -> gtk4::Entry {
    let e = gtk4::Entry::new();
    e.set_text(text);
    e.set_hexpand(true);
    let last = Rc::new(RefCell::new(text.to_string()));
    let commit = Rc::new(move |e: &gtk4::Entry| {
        let t = e.text().to_string();
        if *last.borrow() != t && put(t.clone().into()) {
            *last.borrow_mut() = t;
        }
    });
    let c = commit.clone();
    e.connect_activate(move |e| c(e));
    let focus = gtk4::EventControllerFocus::new();
    let e2 = e.clone();
    focus.connect_leave(move |_| commit(&e2));
    e.add_controller(focus);
    e
}

/// A colour as the config writes it, #rrggbb.
pub fn hex(c: &gtk4::gdk::RGBA) -> String {
    let b = |x: f32| (x * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c.red()), b(c.green()), b(c.blue()))
}

/// The control for a field, and whether it goes under its title (wide) rather than beside it.
fn control(f: &Field, cur: &Value, secret_key: &str, put: Put, err: &gtk4::Label) -> (gtk4::Widget, bool) {
    let text = cur.as_str().unwrap_or("");
    match &f.kind {
        Kind::Bool => {
            let s = gtk4::Switch::new();
            s.set_active(cur.as_bool().unwrap_or(false));
            s.connect_active_notify(move |s| {
                put(s.is_active().into());
            });
            (s.upcast(), false)
        }
        &Kind::Number { min, max, step, slider } => {
            let put = settled(put);
            let n = cur.as_f64().unwrap_or(min);
            if slider {
                let sc = gtk4::Scale::with_range(Orientation::Horizontal, min, max, step);
                sc.set_value(n);
                sc.set_draw_value(true);
                sc.set_value_pos(gtk4::PositionType::Right);
                sc.set_digits(if step.fract() == 0.0 { 0 } else { 2 });
                sc.set_hexpand(true);
                // on the step, without a float's tail (0.85, not 0.8500000000000001)
                sc.connect_value_changed(move |s| {
                    put((((s.value() / step).round() * step * 1e6).round() / 1e6).into());
                });
                (sc.upcast(), true)
            } else {
                let sp = gtk4::SpinButton::with_range(min, max, step);
                sp.set_value(n);
                sp.connect_value_changed(move |s| put(s.value().into()));
                (sp.upcast(), false)
            }
        }
        Kind::Choice { options: opts } | Kind::Multi { options: opts } => {
            let multi = matches!(f.kind, Kind::Multi { .. });
            let values: Vec<String> = opts.iter().map(|o| o.value().to_string()).collect();
            let picked: Vec<bool> = values
                .iter()
                .map(|v| if multi { cur.as_array().is_some_and(|a| a.iter().any(|x| x == v)) } else { v == text })
                .collect();
            let names: Vec<&str> = opts.iter().map(|o| o.label()).collect();
            let on = Rc::new(RefCell::new(picked.clone()));
            let fb = options(&names, &picked, multi, move |i, now| {
                if !multi {
                    put(values[i].clone().into());
                    return;
                }
                on.borrow_mut()[i] = now;
                let all: Vec<Value> =
                    on.borrow().iter().zip(&values).filter(|(o, _)| **o).map(|(_, v)| v.clone().into()).collect();
                put(all.into());
            });
            (fb.upcast(), true)
        }
        Kind::List => (list(cur, put).upcast(), true),
        Kind::Secret => (secret(secret_key, f, err).upcast(), true),
        Kind::Duration => {
            let e = entry(&cur.as_u64().map(fmt_duration).unwrap_or_default(), put);
            e.set_hexpand(false);
            e.set_width_chars(8);
            (e.upcast(), false)
        }
        Kind::Color => {
            let bx = gtk4::Box::new(Orientation::Horizontal, 6);
            let b = gtk4::ColorDialogButton::new(Some(gtk4::ColorDialog::new()));
            // none set: a clear swatch, not the dialog's own red
            b.set_rgba(&gtk4::gdk::RGBA::parse(text).unwrap_or(gtk4::gdk::RGBA::TRANSPARENT));
            let e = {
                let b = b.clone();
                entry(text, Rc::new(move |v| {
                    let ok = put(v.clone());
                    if let Some(c) = v.as_str().and_then(|t| gtk4::gdk::RGBA::parse(t).ok()).filter(|_| ok) {
                        b.set_rgba(&c);
                    }
                    ok
                }))
            };
            e.set_placeholder_text(Some("#rrggbb"));
            let e2 = e.clone();
            // the dialog's pick into the entry, written as if typed there
            b.connect_rgba_notify(move |b| {
                if gtk4::gdk::RGBA::parse(e2.text().as_str()).ok().as_ref() != Some(&b.rgba()) {
                    e2.set_text(&hex(&b.rgba()));
                    e2.emit_activate();
                }
            });
            bx.append(&e);
            bx.append(&b);
            (bx.upcast(), true)
        }
        Kind::String | Kind::Url | Kind::Path => {
            let e = entry(text, put);
            if f.kind == Kind::Path {
                e.set_placeholder_text(Some("~/…"));
            }
            (e.upcast(), true)
        }
    }
}

/// A list of strings: each with its minus, an entry under them adding one on Enter.
fn list(cur: &Value, put: Put) -> gtk4::Box {
    let items = cur.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from));
    let items = Rc::new(RefCell::new(items.collect::<Vec<_>>()));
    let col = gtk4::Box::new(Orientation::Vertical, 2);
    let rows = gtk4::Box::new(Orientation::Vertical, 2);
    col.append(&rows);
    let add = gtk4::Entry::new();
    add.set_placeholder_text(Some("Add…"));
    col.append(&add);
    fn draw(rows: &gtk4::Box, items: &Rc<RefCell<Vec<String>>>, put: &Put) {
        clear(rows);
        for (i, it) in items.borrow().iter().enumerate() {
            let line = gtk4::Box::new(Orientation::Horizontal, 6);
            let t = label(it, "");
            t.set_hexpand(true);
            t.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            let minus = gtk4::Button::from_icon_name("list-remove-symbolic");
            minus.add_css_class("flat-round");
            let (rows2, items2, put2) = (rows.clone(), items.clone(), put.clone());
            minus.connect_clicked(move |_| {
                let was = items2.borrow_mut().remove(i);
                if !put2(Value::from(items2.borrow().clone())) {
                    items2.borrow_mut().insert(i, was);
                }
                let (rows, items, put) = (rows2.clone(), items2.clone(), put2.clone());
                // drawn anew once this button's click is over
                glib::idle_add_local_once(move || draw(&rows, &items, &put));
            });
            line.append(&t);
            line.append(&minus);
            rows.append(&line);
        }
    }
    draw(&rows, &items, &put);
    add.connect_activate(move |e| {
        let t = e.text().trim().to_string();
        if t.is_empty() {
            return;
        }
        items.borrow_mut().push(t);
        if put(Value::from(items.borrow().clone())) {
            e.set_text("");
            draw(&rows, &items, &put);
        } else {
            items.borrow_mut().pop();
        }
    });
    col
}

/// A secret: typed and Enter keeps it in the keyring (empty and Enter forgets it); never shown, only whether
/// one is kept.
fn secret(key: &str, f: &Field, err: &gtk4::Label) -> gtk4::PasswordEntry {
    let e = gtk4::PasswordEntry::new();
    e.set_show_peek_icon(true);
    e.set_hexpand(true);
    let said = |e: &gtk4::PasswordEntry, kept: bool| {
        e.set_placeholder_text(Some(if kept { "Kept in the keyring; type to replace" } else { "Not set" }));
    };
    let (k, e2) = (key.to_string(), e.clone());
    off_thread(move || super::secret::exists(&k), move |kept| said(&e2, kept));
    let table = key.rsplit_once('.').map_or("", |s| s.0).to_string();
    let (k, err, field) = (key.to_string(), err.clone(), f.key.clone());
    e.connect_activate(move |e| {
        let t = e.text().to_string();
        let (k, e, err, table, field) = (k.clone(), e.clone(), err.clone(), table.clone(), field.clone());
        e.set_sensitive(false);
        let t2 = t.clone();
        off_thread(move || super::secret::store(&k, (!t2.is_empty()).then_some(t2.as_str())), move |r| {
            e.set_sensitive(true);
            match r {
                Ok(()) => {
                    e.set_text("");
                    said(&e, !t.is_empty());
                    // one written in the file by hand before goes
                    let _ = write(&table, &field, None, false);
                    err.set_visible(false);
                }
                Err(m) => {
                    err.set_text(&m);
                    err.set_visible(true);
                }
            }
        });
    });
    e
}

/// The groups of the Settings list, by an entry's id.
fn group(id: &str) -> &'static str {
    match id {
        "appearance" | "bar" | "idle" | "calendar" | "games" => "ostrov",
        id if id.starts_with("widget.") => "Widgets",
        _ => "Plugins",
    }
}

/// The control centre's Settings page: its header, the entries' list, an entry's form in its place.
pub struct Page {
    pub root: gtk4::Box,
    title: gtk4::Label,
    stack: gtk4::Stack,
    list: gtk4::Box,
    form: gtk4::Box,
    current: RefCell<Option<String>>,
    back: Box<dyn Fn()>,
    route: Box<dyn Fn(&str) -> bool>,
}

impl Page {
    /// The page; back leaves it, route takes an entry drawn elsewhere (Appearance's own page), saying so.
    pub fn new(back: impl Fn() + 'static, route: impl Fn(&str) -> bool + 'static) -> Rc<Page> {
        let root = gtk4::Box::new(Orientation::Vertical, 6);
        let me: Rc<RefCell<std::rc::Weak<Page>>> = Rc::default();
        let m = me.clone();
        let (head, title) = header("Settings", move || {
            if let Some(p) = m.borrow().upgrade() {
                p.back();
            }
        });
        root.append(&head);
        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);
        stack.set_transition_duration(120);
        stack.set_vhomogeneous(false);
        stack.set_interpolate_size(true);
        let list = gtk4::Box::new(Orientation::Vertical, 2);
        let form = gtk4::Box::new(Orientation::Vertical, 0);
        stack.add_named(&list, Some("list"));
        stack.add_named(&form, Some("form"));
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_hscrollbar_policy(gtk4::PolicyType::Never);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(560);
        stack.add_css_class("page-body");
        scroll.set_child(Some(&stack));
        root.append(&scroll);
        let p = Rc::new(Page {
            root,
            title,
            stack,
            list,
            form,
            current: RefCell::default(),
            back: Box::new(back),
            route: Box::new(route),
        });
        *me.borrow_mut() = Rc::downgrade(&p);
        p.fill_list();
        let w = Rc::downgrade(&p);
        super::on_register(move || {
            if let Some(p) = w.upgrade() {
                p.fill_list();
            }
        });
        // the file edited by hand (or by bin/theme): the form open drawn again from it
        let w = Rc::downgrade(&p);
        super::on_outside(move || {
            if let Some((p, id)) = w.upgrade().and_then(|p| p.current.borrow().clone().map(|id| (p.clone(), id))) {
                p.fill_form(&id);
            }
        });
        p
    }

    /// The list (None), or an entry's form; an id not known, the list.
    pub fn show(&self, id: Option<&str>) {
        let Some(id) = id.filter(|id| entries().iter().any(|e| e.id == *id)) else {
            *self.current.borrow_mut() = None;
            self.title.set_text("Settings");
            self.stack.set_visible_child_name("list");
            return;
        };
        if (self.route)(id) {
            return;
        }
        self.fill_form(id);
        self.stack.set_visible_child_name("form");
    }

    fn back(&self) {
        if self.current.borrow().is_some() {
            self.show(None);
        } else {
            (self.back)();
        }
    }

    fn fill_form(&self, id: &str) {
        let Some(e) = entries().into_iter().find(|e| e.id == id) else { return };
        *self.current.borrow_mut() = Some(id.to_string());
        self.title.set_text(&e.title);
        clear(&self.form);
        self.form.append(&form(&e.schema));
    }

    fn fill_list(self: &Rc<Self>) {
        clear(&self.list);
        let mut all = entries();
        all.sort_by_key(|e| ["ostrov", "Widgets"].iter().position(|g| *g == group(&e.id)).unwrap_or(2));
        let mut last = "ostrov";
        for e in all {
            let g = group(&e.id);
            if g != last {
                let t = label(g, "dim");
                t.add_css_class("form-section");
                self.list.append(&t);
                last = g;
            }
            let (me, id) = (Rc::downgrade(self), e.id.clone());
            self.list.append(&row(&e.icon, &e.title, "", false, move || {
                if let Some(p) = me.upgrade() {
                    p.show(Some(&id));
                }
            }));
        }
    }
}
