//! Widgets of the user's own, written in KDL with no code (docs/widgets.md): ~/.config/ostrov/widgets/*.kdl,
//! each file one or more widgets joining the panels' registry and gallery under their ids. A widget is fed by
//! commands, polled or left running (a value a line), all on GTK's main loop through GIO's subprocesses, never
//! blocking it; its nodes, plugins' nodes (plugins/node.rs), are rendered from those values, the desktop's state
//! and its settings ([widget.ID]) on every draw, and drawn by the plugins' renderer, in place. What its nodes'
//! events run goes through sh, every value put into it quoted. A file edited is read again and its widgets drawn
//! anew at once; a widget new to it waits for `ostrov restart`. A file that does not read is said with its line on
//! stderr and once in a toast.

mod decl;
mod expr;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use futures_util::future::{select, Either};
use gtk4::prelude::*;
use gtk4::{gio, glib};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::cc::{Ctx, Meta};
use crate::plugins::{node, View};
use decl::{Decl, Handlers};
use expr::Scope;

/// A widget declared, live: its declaration (replaced as its file is edited), its sources' values, its settings,
/// the state last drawn with, its views on the grids.
struct Live {
    file: PathBuf,
    decl: RefCell<Decl>,
    values: RefCell<Map<String, Value>>,
    config: RefCell<Value>,
    state: RefCell<Value>,
    /// its sources' generation: a loop of an older one ends
    generation: Cell<u64>,
    started: Cell<bool>,
    listening: RefCell<Vec<gio::Subprocess>>,
    views: RefCell<Vec<Weak<Shown>>>,
}

/// One of its views, and what its nodes' events run, as last rendered.
struct Shown {
    view: Rc<View>,
    handlers: RefCell<Handlers>,
}

thread_local! {
    static LIVE: RefCell<Vec<Rc<Live>>> = const { RefCell::new(Vec::new()) };
    /// what was last said of each file that did not read, said once
    static SAID: RefCell<HashMap<PathBuf, String>> = RefCell::default();
}

fn dir() -> PathBuf {
    crate::hub::home().join(".config/ostrov/widgets")
}

/// What a command printed: JSON if it is, else its text trimmed.
fn output(s: &str) -> Value {
    serde_json::from_str(s.trim()).unwrap_or_else(|_| json!(s.trim()))
}

/// A file's widgets, or what is wrong with it said: on stderr, and in a toast unless it was just said.
fn read(path: &Path) -> Option<Vec<Decl>> {
    let text = std::fs::read_to_string(path).map_err(|e| eprintln!("ostrov: {}: {e}", path.display())).ok()?;
    match decl::parse(&text) {
        Ok(ds) => {
            SAID.with(|s| s.borrow_mut().remove(path));
            Some(ds)
        }
        Err((line, msg)) => {
            let what = format!("{}:{line}: {msg}", path.display());
            eprintln!("ostrov: {what}");
            if SAID.with(|s| s.borrow_mut().insert(path.into(), what.clone())) != Some(what.clone()) {
                // the notifications may not be up yet as the first panel is built
                glib::idle_add_local_once(move || {
                    if let Some(n) = crate::notes::get() {
                        n.post("dialog-warning-symbolic", "A widget file does not read", &what, false);
                    }
                });
            }
            None
        }
    }
}

/// A command run through sh, its output; one taking more than a minute ended.
async fn run(cmd: &str) -> Option<Value> {
    let argv = [OsStr::new("sh"), OsStr::new("-c"), OsStr::new(cmd)];
    let p = gio::Subprocess::newv(&argv, gio::SubprocessFlags::STDOUT_PIPE)
        .map_err(|e| eprintln!("ostrov: widgets: {cmd}: {e}"))
        .ok()?;
    match select(p.communicate_utf8_future(None), glib::timeout_future(Duration::from_secs(60))).await {
        Either::Left((Ok((out, _)), _)) => Some(output(out.as_deref().unwrap_or(""))),
        Either::Left((Err(e), _)) => {
            eprintln!("ostrov: widgets: {cmd}: {e}");
            None
        }
        Either::Right(_) => {
            eprintln!("ostrov: widgets: {cmd}: no end in a minute, ended");
            p.force_exit();
            None
        }
    }
}

impl Live {
    fn log(&self, what: &str) {
        eprintln!("ostrov: widget {} ({}): {what}", self.decl.borrow().id, self.file.display());
    }

    /// What its expressions see: its sources' values, the state, its settings.
    fn globals(&self) -> Map<String, Value> {
        let mut g = self.values.borrow().clone();
        g.insert("state".into(), self.state.borrow().clone());
        g.insert("config".into(), self.config.borrow().clone());
        g
    }

    /// Its settings as the config has them, their defaults where it says nothing.
    fn read_config(&self) {
        let d = self.decl.borrow();
        let mut c: Map<String, Value> = d.settings.iter().map(|f| (f.key.clone(), f.default.clone())).collect();
        let table = crate::config::load().widget.remove(&d.id).and_then(|t| serde_json::to_value(t).ok());
        if let Some(Value::Object(t)) = table {
            c.extend(t);
        }
        *self.config.borrow_mut() = Value::Object(c);
    }

    /// A command of its own, its values quoted for sh.
    fn command(&self, exec: &str, locals: Vec<(String, Value)>) -> Option<String> {
        let g = self.globals();
        match expr::interp(exec, &Scope { globals: &g, locals }, true) {
            Ok(Value::String(c)) => Some(c),
            _ => None,
        }
    }

    /// Its views drawn again (their panels, which draw them).
    fn changed(&self) {
        self.views.borrow_mut().retain(|v| v.upgrade().is_some());
        let views: Vec<Rc<Shown>> = self.views.borrow().iter().filter_map(Weak::upgrade).collect();
        for v in views {
            (v.view.again)();
        }
    }

    fn set(&self, name: &str, v: Value) {
        if self.values.borrow().get(name) != Some(&v) {
            self.values.borrow_mut().insert(name.into(), v);
            self.changed();
        }
    }

    /// A source of its, by name, as the declaration has it now.
    fn source(&self, name: &str) -> Option<decl::Source> {
        self.decl.borrow().sources.iter().find(|s| s.name == name).cloned()
    }

    /// Its sources started anew: those of before ended, the listens killed.
    fn start(self: &Rc<Self>) {
        self.started.set(true);
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        self.listening.borrow_mut().drain(..).for_each(|p| p.force_exit());
        for s in self.decl.borrow().sources.clone() {
            let me = self.clone();
            match s.every {
                Some(_) => glib::spawn_future_local(async move { me.poll(&s.name, generation).await }),
                None => glib::spawn_future_local(async move { me.listen(&s.name, generation).await }),
            };
        }
    }

    async fn poll(self: Rc<Self>, name: &str, generation: u64) {
        while self.generation.get() == generation {
            let Some(src) = self.source(name) else { return };
            self.poll_once(&src).await;
            glib::timeout_future(Duration::from_secs(src.every.unwrap_or(60))).await;
        }
    }

    async fn poll_once(&self, src: &decl::Source) {
        let Some(cmd) = self.command(&src.exec, Vec::new()) else { return };
        if let Some(v) = run(&cmd).await {
            self.set(&src.name, v);
        }
    }

    /// A command left running, each line it prints the source's value; started again when it ends, 1 s later,
    /// twice as long each time it ends within a minute, up to a minute.
    async fn listen(self: Rc<Self>, name: &str, generation: u64) {
        let mut wait = Duration::from_secs(1);
        while self.generation.get() == generation {
            let Some(cmd) = self.source(name).and_then(|s| self.command(&s.exec, Vec::new())) else { return };
            let started = Instant::now();
            let argv = [OsStr::new("sh"), OsStr::new("-c"), OsStr::new(&cmd)];
            match gio::Subprocess::newv(&argv, gio::SubprocessFlags::STDOUT_PIPE) {
                Ok(p) => {
                    self.listening.borrow_mut().push(p.clone());
                    if let Some(out) = p.stdout_pipe() {
                        let lines = gio::DataInputStream::new(&out);
                        while let Ok(Some(l)) = lines.read_line_utf8_future(glib::Priority::DEFAULT).await {
                            if self.generation.get() != generation {
                                break;
                            }
                            self.set(name, output(&l));
                        }
                    }
                    let _ = p.wait_future().await;
                    self.listening.borrow_mut().retain(|q| q != &p);
                }
                Err(e) => self.log(&format!("{cmd}: {e}")),
            }
            if self.generation.get() != generation {
                return;
            }
            if started.elapsed() > Duration::from_secs(60) {
                wait = Duration::from_secs(1);
            }
            self.log(&format!("listen {name} ended, again in {} s", wait.as_secs()));
            glib::timeout_future(wait).await;
            wait = (wait * 2).min(Duration::from_secs(60));
        }
    }

    /// A view drawn from its declaration now: its tree, its badge, what its events run.
    fn render(self: &Rc<Self>, shown: &Rc<Shown>, state: &Value) {
        *self.state.borrow_mut() = state.clone();
        let g = self.globals();
        let mut sc = Scope { globals: &g, locals: Vec::new() };
        let mut hs = Handlers::new();
        let d = self.decl.borrow();
        let tree = decl::one(decl::render(&d.body, &mut sc, &mut hs));
        *shown.handlers.borrow_mut() = hs;
        let (me, s) = (Rc::downgrade(self), Rc::downgrade(shown));
        let emit: node::Emit = Rc::new(move |node, event, value| {
            if let (Some(me), Some(s)) = (me.upgrade(), s.upgrade()) {
                me.fire(&s, node, event, value);
            }
        });
        match node::El::deserialize(&tree) {
            Ok(el) => shown.view.paint(&el, &emit),
            Err(e) => self.log(&format!("{e}: {tree}")),
        }
        if let Some(b) = &d.badge {
            let (tree, active) = decl::badge(b, &sc);
            if let Ok(el) = node::El::deserialize(&tree) {
                shown.view.paint_badge(&el, active, &emit);
            }
        }
    }

    /// A node's event: each exec of its for that event run through sh, the event's value as {value} beside
    /// the loop's variables; its polls polled again once it ends (or in 2 s, if it goes on).
    fn fire(self: &Rc<Self>, shown: &Shown, node: &str, event: &str, value: String) {
        let cmds: Vec<String> = {
            let hs = shown.handlers.borrow();
            let Some(h) = hs.get(node) else { return };
            let mut locals = h.locals.clone();
            locals.push(("value".into(), json!(value)));
            h.events
                .iter()
                .filter(|(e, _)| decl::matches(e, event))
                .filter_map(|(_, exec)| self.command(exec, locals.clone()))
                .collect()
        };
        for cmd in cmds {
            let me = self.clone();
            glib::spawn_future_local(async move {
                let argv = [OsStr::new("sh"), OsStr::new("-c"), OsStr::new(&cmd)];
                match gio::Subprocess::newv(&argv, gio::SubprocessFlags::NONE) {
                    Ok(p) => {
                        let _ = select(p.wait_future(), glib::timeout_future(Duration::from_secs(2))).await;
                    }
                    Err(e) => return me.log(&format!("{cmd}: {e}")),
                }
                let polls: Vec<decl::Source> =
                    me.decl.borrow().sources.iter().filter(|s| s.every.is_some()).cloned().collect();
                for s in polls {
                    me.poll_once(&s).await;
                }
            });
        }
    }

    /// It made for a grid; its sources started with its first view.
    fn widget(self: &Rc<Self>, c: &Ctx) -> crate::cc::Widget {
        let (view, mut w) = {
            let d = self.decl.borrow();
            View::make(&d.id, &d.name, &d.icon, d.sizes[0], d.badge.is_some(), c)
        };
        let shown = Rc::new(Shown { view, handlers: RefCell::default() });
        self.views.borrow_mut().push(Rc::downgrade(&shown));
        let me = self.clone();
        w.draw = Box::new(move |st| me.render(&shown, st));
        if !self.started.get() {
            self.start();
        }
        w
    }
}

/// Its settings' form on the Settings page, as widget.ID, if it has any.
fn register(d: &Decl) {
    if !d.settings.is_empty() {
        let section = crate::settings::Section::new("", &d.name, d.settings.clone());
        let schema = crate::settings::Schema { sections: vec![section] };
        crate::settings::register(&format!("widget.{}", d.id), &d.name, &d.icon, schema);
    }
}

/// The files, sorted, that the widgets are in.
fn files() -> Vec<PathBuf> {
    let mut fs: Vec<PathBuf> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension() == Some(OsStr::new("kdl")))
        .collect();
    fs.sort();
    fs
}

/// A file edited: its widgets' declarations replaced, their sources started anew if those changed, drawn again.
fn reload(path: &Path) {
    let Some(ds) = read(path) else { return };
    for d in ds {
        let Some(live) = LIVE.with(|l| l.borrow().iter().find(|l| l.decl.borrow().id == d.id).cloned()) else {
            eprintln!("ostrov: {}: widget {} new: there after ostrov restart", path.display(), d.id);
            continue;
        };
        if live.file != path {
            continue;
        }
        let restart = live.decl.borrow().sources != d.sources;
        register(&d);
        *live.decl.borrow_mut() = d;
        live.read_config();
        if restart && live.started.get() {
            live.start();
        }
        live.changed();
    }
}

/// The widgets of the files for the panels' registry, each under its own id; one whose id is taken (by
/// ostrov's, a plugin's, another file's) said and left out. Their directory watched for edits.
pub fn metas(taken: &[&str]) -> Vec<Meta> {
    // ponytail: the registry's names are 'static, so these are leaked, once, as the registry is made
    let leak = |s: &str| -> &'static str { Box::leak(s.to_string().into_boxed_str()) };
    let mut all = Vec::new();
    for f in files() {
        for d in read(&f).unwrap_or_default() {
            if taken.contains(&d.id.as_str()) || all.iter().any(|m: &Meta| m.id == d.id) {
                eprintln!("ostrov: {}: widget id {:?} taken, left out", f.display(), d.id);
                continue;
            }
            register(&d);
            let live = Rc::new(Live {
                file: f.clone(),
                values: RefCell::default(),
                config: RefCell::default(),
                state: RefCell::default(),
                generation: Cell::new(0),
                started: Cell::new(false),
                listening: RefCell::default(),
                views: RefCell::default(),
                decl: RefCell::new(d.clone()),
            });
            live.read_config();
            LIVE.with(|l| l.borrow_mut().push(live.clone()));
            all.push(Meta {
                id: leak(&d.id),
                name: leak(&d.name),
                icon: leak(&d.icon),
                sizes: Box::leak(d.sizes.into_boxed_slice()),
                make: Rc::new(move |c| live.widget(c)),
                settings: None,
                bar: d.bar,
            });
        }
    }
    watch();
    all
}

/// The files' directory watched: a file saved read again; the config's change their settings read again.
fn watch() {
    if !dir().is_dir() {
        return;
    }
    match gio::File::for_path(dir()).monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
        Ok(mon) => {
            mon.connect_changed(|_, file, _, ev| {
                let saved = matches!(ev, gio::FileMonitorEvent::ChangesDoneHint | gio::FileMonitorEvent::Created);
                if let Some(p) = file.path().filter(|p| saved && p.extension() == Some(OsStr::new("kdl"))) {
                    reload(&p);
                }
            });
            // kept for the program's life
            std::mem::forget(mon);
        }
        Err(e) => eprintln!("ostrov: {}: {e}", dir().display()),
    }
    crate::style::on_config(|| {
        LIVE.with(|l| {
            for live in l.borrow().iter() {
                let before = live.config.borrow().clone();
                live.read_config();
                if *live.config.borrow() != before {
                    live.changed();
                }
            }
        })
    });
}
