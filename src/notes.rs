//! Notifications, rbar being the notification server (org.freedesktop.Notifications over zbus, in its own Tokio
//! thread). Toasts at the top right under the bar: the app, the summary, the body, the actions as buttons; a
//! click runs the default action, a right click dismisses; gone after their timeout (5 s unless they say), a
//! critical one stays. What bin/wm-fnkeys sends (app "fnkeys": a level in its "value" hint, or a word, its icon
//! as the image) is the OSD instead, at the bottom centre. The rest stays in the history (the calendar's) until
//! dismissed; Do Not Disturb keeps the toasts back, not the history.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
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
}

struct Server {
    tx: async_channel::Sender<In>,
    next: std::sync::atomic::AtomicU32,
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
        vec!["body".into(), "actions".into(), "persistence".into()]
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
        let id = if replaces_id != 0 { replaces_id } else { self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) };
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
        ("rbar".into(), "dotfiles".into(), "0.1".into(), "1.2".into())
    }

    #[zbus(signal)]
    async fn notification_closed(e: &SignalEmitter<'_>, id: u32, reason: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(e: &SignalEmitter<'_>, id: u32, action_key: String) -> zbus::Result<()>;
}

/// The server in its own Tokio runtime; nothing if the name is taken (another notification daemon runs).
fn serve(tx: async_channel::Sender<In>, mut rx: tokio::sync::mpsc::UnboundedReceiver<Out>) {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    rt.block_on(async move {
        let server = Server { tx, next: std::sync::atomic::AtomicU32::new(1) };
        let conn = match zbus::connection::Builder::session()
            .and_then(|b| b.name("org.freedesktop.Notifications"))
            .and_then(|b| b.serve_at("/org/freedesktop/Notifications", server))
        {
            Ok(b) => match b.build().await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("rbar: notifications: {e}");
                    return;
                }
            },
            Err(e) => {
                eprintln!("rbar: notifications: {e}");
                return;
            }
        };
        let Ok(iface) = conn.object_server().interface::<_, Server>("/org/freedesktop/Notifications").await else { return };
        while let Some(out) = rx.recv().await {
            let e = iface.signal_emitter();
            let _ = match out {
                Out::Invoke(id, key) => Server::action_invoked(e, id, key).await,
                Out::Closed(id, reason) => Server::notification_closed(e, id, reason).await,
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
    }
    /// One notification gone: its toast, its place in the history; the app told (reason 2: dismissed).
    pub fn dismiss(&self, id: u32) {
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
    fn invoke(&self, id: u32, key: &str) {
        let _ = self.out.send(Out::Invoke(id, key.to_string()));
        self.dismiss(id);
    }

    /// A notification as a card: the app, the summary, the body, its actions beside the default as buttons; a
    /// click runs the default action, a right click (or any click, in the history) dismisses it.
    pub fn card(self: &Rc<Self>, n: &Note, history: bool) -> gtk4::Box {
        let card = gtk4::Box::new(Orientation::Vertical, 2);
        card.add_css_class("card");
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
        let others: Vec<&(String, String)> = n.actions.iter().filter(|(k, _)| k != "default").collect();
        if !others.is_empty() {
            let row = gtk4::FlowBox::new();
            row.set_selection_mode(gtk4::SelectionMode::None);
            row.set_margin_top(6);
            for (key, text) in others {
                let b = gtk4::Button::with_label(text);
                b.add_css_class("chip");
                let (me, id, key) = (self.clone(), n.id, key.clone());
                b.connect_clicked(move |_| me.invoke(id, &key));
                row.insert(&b, -1);
            }
            card.append(&row);
        }
        let click = gtk4::GestureClick::new();
        click.set_button(0);
        let (me, id, default) = (self.clone(), n.id, n.actions.iter().any(|(k, _)| k == "default"));
        click.connect_released(move |g, _, _, _| {
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

pub fn start(app: &gtk4::Application) -> Rc<Notes> {
    let (in_tx, in_rx) = async_channel::unbounded::<In>();
    let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel::<Out>();
    std::thread::spawn(move || serve(in_tx, out_rx));

    // the toasts' window, top right under the bar, as tall as its toasts
    let toast_win = gtk4::ApplicationWindow::new(app);
    toast_win.init_layer_shell();
    toast_win.set_layer(Layer::Overlay);
    toast_win.set_namespace(Some("rbar-toast"));
    toast_win.set_anchor(Edge::Top, true);
    toast_win.set_anchor(Edge::Right, true);
    toast_win.set_margin(Edge::Top, 4);
    toast_win.set_margin(Edge::Right, 6);
    toast_win.set_default_size(360, -1);
    toast_win.add_css_class("panel-window");
    let toasts = gtk4::Box::new(Orientation::Vertical, 6);
    toasts.add_css_class("toasts");
    toast_win.set_child(Some(&toasts));

    // the OSD, bottom centre: the icon, the words, the level as a bar
    let osd = gtk4::ApplicationWindow::new(app);
    osd.init_layer_shell();
    osd.set_layer(Layer::Overlay);
    osd.set_namespace(Some("rbar-osd"));
    osd.set_anchor(Edge::Bottom, true);
    osd.set_margin(Edge::Bottom, 80);
    osd.set_default_size(300, -1);
    osd.add_css_class("panel-window");
    let obox = gtk4::Box::new(Orientation::Horizontal, 14);
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
    let osd_hide: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();

    let notes = Rc::new(Notes {
        history: RefCell::default(),
        dnd: RefCell::new(false),
        subs: RefCell::default(),
        out: out_tx,
        toasts,
        toast_win,
        shown: RefCell::default(),
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
                    oicon.set_icon_name(Some(note.icon.trim_start_matches("image://icon/")));
                    otext.set_text(&if note.body.is_empty() { note.summary.clone() } else { format!("{}  {}", note.summary, note.body) });
                    olevel.set_visible(note.level.is_some());
                    olevel.set_fraction(note.level.unwrap_or(0) as f64 / 100.0);
                    osd.set_visible(true);
                    if let Some(id) = osd_hide.borrow_mut().take() {
                        id.remove();
                    }
                    let (o2, h2) = (osd.clone(), osd_hide.clone());
                    let ms = if note.timeout > 0 { note.timeout as u64 } else { 1500 };
                    *osd_hide.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_millis(ms), move || {
                        h2.borrow_mut().take();
                        o2.set_visible(false);
                    }));
                }
                In::Notify(note) => {
                    let id = note.id;
                    {
                        let mut h = n.history.borrow_mut();
                        h.retain(|x| x.id != id);
                        h.push(note.clone());
                    }
                    n.unshow(id);
                    if !*n.dnd.borrow() {
                        let card = n.card(&note, false);
                        n.toasts.append(&card);
                        n.shown.borrow_mut().push((id, card.upcast()));
                        n.toast_win.set_visible(true);
                        if !note.critical {
                            let ms = if note.timeout > 0 { note.timeout as u64 } else { 5000 };
                            let n2 = n.clone();
                            glib::timeout_add_local_once(Duration::from_millis(ms), move || n2.unshow(id));
                        }
                    }
                    n.changed();
                }
            }
        }
    });
    notes
}
