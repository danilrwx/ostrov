//! Notifications, ostrov being the notification server (org.freedesktop.Notifications over zbus, in its own Tokio
//! thread). Toasts at the top right under the bar: the app, the summary, the body, the actions as buttons (a chat's
//! answer typed right in it, its inline-reply); a click runs the default action, a right click dismisses; gone
//! after their timeout (5 s unless they say, held while an answer is typed), a critical one stays.
//! What a script of the user's Fn keys sends (app "fnkeys": a level in its "value" hint, or a word, its icon as the
//! image) is the OSD instead, at the bottom centre; ostrov's own keys (keys.rs) show it directly (osd), its own
//! warnings (the battery's) are posted directly too (post). The rest stays in the history (the calendar's) until
//! dismissed; Do Not Disturb keeps the toasts back, not the history.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

#[derive(Clone, Debug)]
pub struct Note {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    pub icon: String,
    /// (key, label), the default one among them
    pub actions: Vec<(String, String)>,
    pub critical: bool,
    pub timeout: i32,
    /// a level 0 to 100, the OSD's bar
    pub level: Option<i32>,
}

/// What the server hands GTK, and what GTK has the server do.
enum In {
    Notify(Note),
    Close(u32),
}
enum Out {
    Invoke(u32, String),
    Closed(u32, u32),
    /// an answer typed into a notification offering one (its "inline-reply" action)
    Reply(u32, String),
}

struct Server {
    tx: async_channel::Sender<In>,
}

/// The next notification's id, the D-Bus clients' and ostrov's own drawn from the same count.
static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn next_id() -> u32 {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn hint_i32(h: &HashMap<String, OwnedValue>, k: &str) -> Option<i32> {
    let v = h.get(k)?;
    i32::try_from(v).ok().or_else(|| u8::try_from(v).ok().map(i32::from)).or_else(|| u32::try_from(v).ok().map(|x| x as i32))
}

fn hint_str(h: &HashMap<String, OwnedValue>, k: &str) -> Option<String> {
    h.get(k).and_then(|v| <&str>::try_from(v).ok().map(String::from))
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn get_capabilities(&self) -> Vec<String> {
        // inline-reply: a chat's notification answered in its toast (KDE's and GNOME's extension, Telegram's)
        vec!["body".into(), "actions".into(), "persistence".into(), "inline-reply".into()]
    }

    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 { replaces_id } else { next_id() };
        let icon = if app_icon.is_empty() { hint_str(&hints, "image-path").unwrap_or_default() } else { app_icon };
        let note = Note {
            id,
            app: app_name,
            summary,
            body,
            icon,
            actions: actions.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].clone(), c[1].clone())).collect(),
            critical: hint_i32(&hints, "urgency") == Some(2),
            timeout: expire_timeout,
            level: hint_i32(&hints, "value"),
        };
        let _ = self.tx.send(In::Notify(note)).await;
        id
    }

    async fn close_notification(&self, id: u32) {
        let _ = self.tx.send(In::Close(id)).await;
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        ("ostrov".into(), "ostrov".into(), "0.1".into(), "1.2".into())
    }

    #[zbus(signal)]
    async fn notification_closed(e: &SignalEmitter<'_>, id: u32, reason: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(e: &SignalEmitter<'_>, id: u32, action_key: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn notification_replied(e: &SignalEmitter<'_>, id: u32, text: String) -> zbus::Result<()>;
}

/// The server in its own Tokio runtime; nothing if the name is taken (another notification daemon runs).
fn serve(tx: async_channel::Sender<In>, mut rx: tokio::sync::mpsc::UnboundedReceiver<Out>) {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
    rt.block_on(async move {
        let server = Server { tx };
        let conn = match zbus::connection::Builder::session()
            .and_then(|b| b.name("org.freedesktop.Notifications"))
            .and_then(|b| b.serve_at("/org/freedesktop/Notifications", server))
        {
            Ok(b) => match b.build().await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("ostrov: notifications: {e}");
                    return;
                }
            },
            Err(e) => {
                eprintln!("ostrov: notifications: {e}");
                return;
            }
        };
        let Ok(iface) = conn.object_server().interface::<_, Server>("/org/freedesktop/Notifications").await else { return };
        while let Some(out) = rx.recv().await {
            let e = iface.signal_emitter();
            let _ = match out {
                Out::Invoke(id, key) => Server::action_invoked(e, id, key).await,
                Out::Closed(id, reason) => Server::notification_closed(e, id, reason).await,
                Out::Reply(id, text) => Server::notification_replied(e, id, text).await,
            };
        }
    });
}

pub struct Notes {
    history: RefCell<Vec<Note>>,
    dnd: RefCell<bool>,
    subs: RefCell<Vec<Box<dyn Fn()>>>,
    out: tokio::sync::mpsc::UnboundedSender<Out>,
    toasts: gtk4::Box,
    toast_win: gtk4::ApplicationWindow,
    shown: RefCell<Vec<(u32, gtk4::Widget)>>,
    /// the toasts being answered, kept up past their time till the answer goes
    replying: RefCell<std::collections::HashSet<u32>>,
    /// the window focused's class, for quiet while a game is
    focused: RefCell<String>,
    osd: Osd,
}

/// The OSD's window and what it shows: the icon, the words, the level as a bar; hidden again by its timer.
struct Osd {
    win: gtk4::ApplicationWindow,
    icon: gtk4::Image,
    text: gtk4::Label,
    level: gtk4::ProgressBar,
    hide: Rc<RefCell<Option<glib::SourceId>>>,
}

impl Notes {
    pub fn history(&self) -> Vec<Note> {
        self.history.borrow().clone()
    }
    pub fn on_change(&self, f: impl Fn() + 'static) {
        self.subs.borrow_mut().push(Box::new(f));
    }
    fn changed(&self) {
        for f in self.subs.borrow().iter() {
            f();
        }
    }
    pub fn set_dnd(&self, on: bool) {
        *self.dnd.borrow_mut() = on;
        self.changed();
    }
    pub fn dnd(&self) -> bool {
        *self.dnd.borrow()
    }

    /// Whether toasts keep back now: Do Not Disturb, a game focused, the quiet hours ([notifications]).
    pub fn quiet_now(&self) -> bool {
        let cfg = crate::config::load();
        let n = &cfg.notifications;
        *self.dnd.borrow()
            || n.quiet_in_games && crate::modules::games::service::is_game(&self.focused.borrow(), &cfg.games.classes)
            || quiet_hours(&n.quiet_from, &n.quiet_to)
    }

    /// Whether an app's toast keeps back now: quiet, and the app not one let through.
    fn quiet(&self, app: &str) -> bool {
        let allowed = crate::config::load().notifications.allow.iter().any(|a| a.eq_ignore_ascii_case(app));
        !allowed && self.quiet_now()
    }
    /// One notification gone: its toast, its place in the history; the app told (reason 2: dismissed).
    pub fn dismiss(&self, id: u32) {
        self.replying.borrow_mut().remove(&id);
        self.history.borrow_mut().retain(|n| n.id != id);
        self.unshow(id);
        let _ = self.out.send(Out::Closed(id, 2));
        self.changed();
    }
    pub fn clear(&self) {
        let ids: Vec<u32> = self.history.borrow().iter().map(|n| n.id).collect();
        for id in ids {
            self.dismiss(id);
        }
    }
    fn unshow(&self, id: u32) {
        self.shown.borrow_mut().retain(|(i, w)| {
            if *i == id {
                self.toasts.remove(w);
            }
            *i != id
        });
        self.toast_win.set_visible(!self.shown.borrow().is_empty());
    }
    /// The OSD up for ms: an icon (a name), the words, a level 0 to 100 as a bar under them, or none.
    pub fn osd(&self, icon: &str, text: &str, level: Option<i32>, ms: u64) {
        let o = &self.osd;
        o.icon.set_icon_name(Some(icon));
        o.text.set_text(text);
        o.level.set_visible(level.is_some());
        o.level.set_fraction(level.unwrap_or(0).clamp(0, 100) as f64 / 100.0);
        o.win.set_visible(true);
        if let Some(id) = o.hide.borrow_mut().take() {
            id.remove();
        }
        let (win, hide) = (o.win.clone(), o.hide.clone());
        *o.hide.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_millis(ms), move || {
            hide.borrow_mut().take();
            win.set_visible(false);
        }));
    }

    /// One of ostrov's own notifications, a toast and in the history as one over D-Bus would be: an icon, the
    /// summary, the body; critical ones stay until dismissed.
    pub fn post(self: &Rc<Self>, icon: &str, summary: &str, body: &str, critical: bool) {
        let note = Note {
            id: next_id(),
            app: "ostrov".into(),
            summary: summary.into(),
            body: body.into(),
            icon: icon.into(),
            actions: Vec::new(),
            critical,
            timeout: 0,
            level: None,
        };
        self.show(note);
    }

    /// A notification into the history, and up as a toast unless Do Not Disturb.
    fn show(self: &Rc<Self>, note: Note) {
        let id = note.id;
        {
            let mut h = self.history.borrow_mut();
            h.retain(|x| x.id != id);
            h.push(note.clone());
        }
        self.unshow(id);
        if note.critical || !self.quiet(&note.app) {
            let card = self.card(&note, false);
            self.toasts.append(&card);
            self.shown.borrow_mut().push((id, card.upcast()));
            self.toast_win.set_visible(true);
            if !note.critical {
                let ms = if note.timeout > 0 { note.timeout as u64 } else { 5000 };
                let me = self.clone();
                glib::timeout_add_local_once(Duration::from_millis(ms), move || {
                    if !me.replying.borrow().contains(&id) {
                        me.unshow(id);
                    }
                });
            }
        }
        self.changed();
    }

    /// An answer to a notification that offered one, the notification gone then.
    fn reply(&self, id: u32, text: &str) {
        self.replying.borrow_mut().remove(&id);
        let _ = self.out.send(Out::Reply(id, text.to_string()));
        self.dismiss(id);
    }

    fn invoke(&self, id: u32, key: &str) {
        let _ = self.out.send(Out::Invoke(id, key.to_string()));
        self.dismiss(id);
    }

    /// A notification as a card: the app, the summary, the body, its actions beside the default as buttons; a
    /// click runs the default action, a right click (or any click, in the history) dismisses it.
    pub fn card(self: &Rc<Self>, n: &Note, history: bool) -> gtk4::Box {
        let card = gtk4::Box::new(Orientation::Vertical, 2);
        // in the history a card on the calendar, as a toast a surface of its own
        card.add_css_class(if history { "card" } else { "surface" });
        if !history {
            card.add_css_class("toast");
        }
        if n.critical {
            card.add_css_class("critical");
        }
        let lab = |t: &str, c: &str| {
            let l = gtk4::Label::new(Some(t));
            l.set_xalign(0.0);
            l.set_wrap(true);
            l.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
            if !c.is_empty() {
                l.add_css_class(c);
            }
            l
        };
        card.append(&lab(&n.app, "dim"));
        if !n.summary.is_empty() {
            card.append(&lab(&n.summary, "bold"));
        }
        if !n.body.is_empty() {
            let b = lab(&n.body, "");
            b.set_lines(4);
            b.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            card.append(&b);
        }
        let others: Vec<&(String, String)> =
            n.actions.iter().filter(|(k, _)| k != "default" && k != "inline-reply").collect();
        // an answer typed right here, for a chat's notification that offers one
        let reply = n.actions.iter().find(|(k, _)| k == "inline-reply").map(|(_, t)| t.clone());
        // the actions in one row, the answer's chip among them; its entry under them while it is typed in
        if !others.is_empty() || reply.is_some() {
            let row = crate::ui::chip_flow();
            row.set_margin_top(6);
            row.add_css_class("note-actions");
            for (key, text) in others {
                let b = gtk4::Button::with_label(text);
                b.add_css_class("chip");
                let (me, id, key) = (self.clone(), n.id, key.clone());
                b.connect_clicked(move |_| me.invoke(id, &key));
                crate::ui::flow_in(&row, &b);
            }
            card.append(&row);
            if let Some(prompt) = reply {
                let (chip, line) = self.reply_box(n.id, &prompt);
                crate::ui::flow_in(&row, &chip);
                card.append(&line);
            }
        }
        let click = gtk4::GestureClick::new();
        click.set_button(0);
        let (me, id, default) = (self.clone(), n.id, n.actions.iter().any(|(k, _)| k == "default"));
        click.connect_released(move |g, _, x, y| {
            // a click on its actions or its answer theirs, not the card's
            let mut hit = g.widget().and_then(|w| w.pick(x, y, gtk4::PickFlags::DEFAULT));
            while let Some(w) = hit {
                if w.has_css_class("note-actions") || w.has_css_class("note-reply") {
                    return;
                }
                hit = w.parent();
            }
            if g.current_button() == 1 && default && !history {
                me.invoke(id, "default");
            } else {
                me.dismiss(id);
            }
        });
        card.add_controller(click);
        card.set_cursor_from_name(Some("pointer"));
        card
    }
}

impl Notes {
    /// The answer's chip, and the entry it unfolds under the actions: Enter (or Send) sends what is typed; the
    /// toast held up meanwhile.
    fn reply_box(self: &Rc<Self>, id: u32, prompt: &str) -> (gtk4::Button, gtk4::Box) {
        let open = gtk4::Button::with_label(if prompt.is_empty() { "Reply" } else { prompt });
        open.add_css_class("chip");
        let line = gtk4::Box::new(Orientation::Horizontal, 6);
        line.add_css_class("note-reply");
        line.set_margin_top(6);
        line.set_visible(false);
        let entry = gtk4::Entry::new();
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some(if prompt.is_empty() { "Reply…" } else { prompt }));
        let send = gtk4::Button::from_icon_name("go-up-symbolic");
        send.add_css_class("send");
        send.set_tooltip_text(Some("Send"));
        line.append(&entry);
        line.append(&send);
        let (me, l, e) = (self.clone(), line.clone(), entry.clone());
        open.connect_clicked(move |o| {
            me.replying.borrow_mut().insert(id);
            o.set_sensitive(false);
            l.set_visible(true);
            e.grab_focus();
        });
        let go = {
            let (me, e) = (self.clone(), entry.clone());
            Rc::new(move || {
                let text = e.text().trim().to_string();
                if !text.is_empty() {
                    me.reply(id, &text);
                }
            })
        };
        let g = go.clone();
        entry.connect_activate(move |_| g());
        send.connect_clicked(move |_| go());
        (open, line)
    }
}

/// Whether now is between from and to ("23:00", "08:00"; over midnight when from is the later), neither empty.
fn quiet_hours(from: &str, to: &str) -> bool {
    let min = |t: &str| {
        let (h, m) = t.trim().split_once(':')?;
        Some(h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?)
    };
    let (Some(f), Some(t)) = (min(from), min(to)) else { return false };
    let Ok(now) = glib::DateTime::now_local() else { return false };
    within(now.hour() as u32 * 60 + now.minute() as u32, f, t)
}

/// A minute of the day between from and to, the span over midnight when from is the later.
fn within(now: u32, from: u32, to: u32) -> bool {
    if from <= to { from <= now && now < to } else { now >= from || now < to }
}

#[cfg(test)]
mod tests {
    #[test]
    fn quiet_hours_over_midnight() {
        use super::within;
        assert!(within(23 * 60 + 30, 23 * 60, 8 * 60));
        assert!(within(7 * 60, 23 * 60, 8 * 60));
        assert!(!within(12 * 60, 23 * 60, 8 * 60));
        assert!(within(13 * 60, 12 * 60, 14 * 60));
        assert!(!within(14 * 60, 12 * 60, 14 * 60));
    }
}

thread_local!(static NOTES: std::cell::OnceCell<Rc<Notes>> = const { std::cell::OnceCell::new() });

/// The notifications, once started (the panels' notifications widget).
pub fn get() -> Option<Rc<Notes>> {
    NOTES.with(|n| n.get().cloned())
}

pub fn start(app: &gtk4::Application) -> Rc<Notes> {
    let (in_tx, in_rx) = async_channel::unbounded::<In>();
    let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel::<Out>();
    std::thread::spawn(move || serve(in_tx, out_rx));

    // the toasts' window, top right under the bar, as tall as its toasts
    let toast_win = gtk4::ApplicationWindow::new(app);
    toast_win.init_layer_shell();
    toast_win.set_layer(Layer::Overlay);
    toast_win.set_namespace(Some("ostrov-toast"));
    toast_win.set_anchor(Edge::Top, true);
    toast_win.set_anchor(Edge::Right, true);
    toast_win.set_margin(Edge::Top, 4);
    toast_win.set_margin(Edge::Right, 6);
    toast_win.set_default_size(360, -1);
    // the keyboard on a click alone: an answer typed into a toast
    toast_win.set_keyboard_mode(KeyboardMode::OnDemand);
    let toasts = gtk4::Box::new(Orientation::Vertical, 6);
    toast_win.set_child(Some(&toasts));

    // the OSD, bottom centre: the icon, the words, the level as a bar
    let osd = gtk4::ApplicationWindow::new(app);
    osd.init_layer_shell();
    osd.set_layer(Layer::Overlay);
    osd.set_namespace(Some("ostrov-osd"));
    osd.set_anchor(Edge::Bottom, true);
    osd.set_margin(Edge::Bottom, 80);
    osd.set_default_size(300, -1);
    let obox = gtk4::Box::new(Orientation::Horizontal, 14);
    obox.add_css_class("surface");
    obox.add_css_class("osd");
    let oicon = gtk4::Image::new();
    oicon.set_pixel_size(22);
    let ocol = gtk4::Box::new(Orientation::Vertical, 8);
    ocol.set_hexpand(true);
    let otext = gtk4::Label::new(None);
    otext.set_xalign(0.0);
    otext.add_css_class("bold");
    let olevel = gtk4::ProgressBar::new();
    olevel.add_css_class("progress");
    ocol.append(&otext);
    ocol.append(&olevel);
    obox.append(&oicon);
    obox.append(&ocol);
    osd.set_child(Some(&obox));

    let notes = Rc::new(Notes {
        history: RefCell::default(),
        dnd: RefCell::new(false),
        subs: RefCell::default(),
        out: out_tx,
        toasts,
        toast_win,
        shown: RefCell::default(),
        replying: RefCell::default(),
        focused: RefCell::default(),
        osd: Osd { win: osd, icon: oicon, text: otext, level: olevel, hide: Rc::default() },
    });

    let n = notes.clone();
    glib::spawn_future_local(async move {
        while let Ok(msg) = in_rx.recv().await {
            match msg {
                In::Close(id) => {
                    n.history.borrow_mut().retain(|x| x.id != id);
                    n.unshow(id);
                    let _ = n.out.send(Out::Closed(id, 3));
                    n.changed();
                }
                In::Notify(note) if note.app == "fnkeys" => {
                    // the OSD: notify-send -i hands an icon name over as image-path
                    let (s, b) = (&note.summary, &note.body);
                    let text = if b.is_empty() { s.clone() } else { format!("{s}  {b}") };
                    let ms = if note.timeout > 0 { note.timeout as u64 } else { 1500 };
                    n.osd(note.icon.trim_start_matches("image://icon/"), &text, note.level, ms);
                }
                In::Notify(note) => n.show(note),
            }
        }
    });
    let n2 = notes.clone();
    crate::events::on(move |name, payload| {
        if name != "window" {
            return;
        }
        let was = n2.quiet_now();
        *n2.focused.borrow_mut() = payload["class"].as_str().unwrap_or_default().to_string();
        if n2.quiet_now() != was {
            n2.changed();
        }
    });
    NOTES.with(|g| {
        let _ = g.set(notes.clone());
    });
    notes
}
