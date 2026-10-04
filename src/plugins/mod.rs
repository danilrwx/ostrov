//! Plugins: programs putting widgets on the control centre, written in any language (docs/plugins.md). Each
//! lives in ~/.local/share/ostrov/plugins/<id>/ by its manifest.toml, and speaks wit/ostrov-plugin.wit: its
//! exports called by ostrov (state, run, render, on-event, on-timer, on-config, on-state, on-shell-event,
//! calendar-events), ostrov's imports called by it (log, run, ask, secret, http-get, set-timer, kick,
//! set-settings-schema), whatever carries the calls. Its launcher modes ([[launcher]], a prefix typed) are
//! answered by its query and pick; its keys ([[keys]]) bound by hyprland.rs where free; its calendar a source of
//! the calendar's. Installed and removed while ostrov runs by install.rs.
//! Official plugins, ostrov's own built with it (plugins/ in its repository), are read from its packages'
//! share/ostrov/plugins/<id>/ (beside its executable's share/, then XDG_DATA_DIRS's), their binaries beside
//! ostrov's; off until `[plugin.<id>] enabled = true`, and a plugin of the user's by the same id is the one read.
//! Any plugin turns on and off as `enabled` changes, without a restart (its widgets come and go at the next).
//! Its transport is a Backend (backend.rs) that Draws besides: a process talking JSON lines now (process.rs), a
//! WebAssembly component the same way another time. Its widgets are trees of ui.rs's kit (node.rs), pulled with
//! render once it kicks, or pushed; they join the control centre's registry as plugin.<id>.<widget>. Its
//! permissions are kept to where ostrov can: "run" for ostrov's commands, "dialogs" for ask, "network" for
//! http-get, "secrets" for secret, "state" for the desktop's state, "events" for ostrov's events, "keys" for its
//! keys, "calendar" for its calendar; the rest are said, for the user.

mod install;
pub mod node;
mod process;

pub use install::is_url;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::{Arc, OnceLock};

use gtk4::prelude::*;
use gtk4::{gio, glib};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::backend::{Backend, BoxFut};
use crate::cc::{Ctx, Face, Meta, Show, Widget};
use crate::hub::Hub;

/// The protocol's version this ostrov speaks: a plugin asking for a later one is not run.
const API: u32 = 1;

#[derive(Deserialize, Debug, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    pub api: u32,
    pub exec: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub widgets: Vec<WidgetDecl>,
    #[serde(default)]
    pub commands: Vec<CommandDecl>,
    #[serde(default)]
    pub launcher: Vec<ModeDecl>,
    /// ostrov's events it follows (src/events.rs's names, "*" for all), with permission "events"
    #[serde(default)]
    pub events: Vec<String>,
    /// its keys, with permission "keys"
    #[serde(default)]
    pub keys: Vec<KeyDecl>,
    /// a calendar for the calendar's events, with permission "calendar"
    #[serde(default)]
    pub calendar: bool,
}

impl Manifest {
    fn allows(&self, what: &str) -> bool {
        self.permissions.iter().any(|p| p == what)
    }

    /// Whether it is sent an event of that name.
    fn follows(&self, event: &str) -> bool {
        self.allows("events") && self.events.iter().any(|e| e == "*" || e == event)
    }

    /// Its keys for Hyprland, as hyprland.rs takes them: none without "keys".
    fn hypr_keys(&self) -> Vec<crate::modules::hyprland::PluginKey> {
        let keys = self.keys.iter().filter(|_| self.allows("keys"));
        keys.map(|k| (self.id.clone(), k.combo.trim().to_string(), k.command.trim().to_string())).collect()
    }
}

/// One of its keys: a combination as Hyprland's binds write it ("SUPER, F12"), bound where free to
/// `ostrov plugin <id> <command>`.
#[derive(Deserialize, Debug, PartialEq)]
pub struct KeyDecl {
    pub combo: String,
    /// its own command's words
    pub command: String,
}

/// One of its launcher modes: what is typed starting with prefix is the plugin's to answer, its name the
/// launcher's prompt.
#[derive(Deserialize, Debug, PartialEq, Clone)]
pub struct ModeDecl {
    pub prefix: String,
    pub name: String,
    #[serde(default)]
    pub icon: String,
}

/// A hit a launcher mode answers with: a row of the launcher's. Picking it opens open, else copies copy, else
/// calls the plugin's pick with its id.
#[derive(Deserialize, Debug, PartialEq, Clone, Default)]
pub struct ModeHit {
    #[serde(default)]
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub icon: String,
    pub open: Option<String>,
    pub copy: Option<String>,
}

/// What query answered, as hits: null for none, a hit that does not read passed over.
fn parse_hits(v: Value) -> Result<Vec<ModeHit>, String> {
    match v {
        Value::Null => Ok(vec![]),
        Value::Array(all) => Ok(all.into_iter().filter_map(|h| ModeHit::deserialize(h).ok()).collect()),
        v => Err(format!("not a list of hits: {v}")),
    }
}

/// One of its commands, `ostrov plugin <id> <usage>`, for help: the words are the plugin's to read.
#[derive(Deserialize, Debug, PartialEq)]
pub struct CommandDecl {
    pub name: String,
    /// its words after the id in forms.rs's grammar ("set N", "mode on|off"), its name if left out
    #[serde(default)]
    pub usage: String,
    #[serde(default)]
    pub help: String,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct WidgetDecl {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: String,
    /// the sizes it allows in cells, [w, h], the first its default
    #[serde(default)]
    pub sizes: Vec<(u8, u8)>,
    /// a badge in its panel's face in the bar, rendered as render("<id>#bar")
    #[serde(default)]
    pub badge: bool,
    /// when the badge shows unless the panel says otherwise
    #[serde(default = "never")]
    pub bar: Show,
}

fn never() -> Show {
    Show::Never
}

/// Whether s is an id's word, of [a-z0-9-].
fn word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `ostrov plugin`'s words of its own, no plugin's id.
const RESERVED: &[&str] = &["install", "remove"];

/// A manifest read and checked: its id its directory's name (so one plugin to an id), of [a-z0-9-] (it goes
/// into the command line, the widgets' ids and the config's section) and none of RESERVED, an API this ostrov
/// speaks, keys at combinations, sizes within the grid.
fn parse_manifest(text: &str, dir: &str) -> Result<Manifest, String> {
    let mut m: Manifest = toml::from_str(text).map_err(|e| e.to_string())?;
    if m.id != dir || !word(&m.id) {
        return Err(format!("id {:?}: not its directory's name, or not of [a-z0-9-]", m.id));
    }
    if RESERVED.contains(&m.id.as_str()) {
        return Err(format!("id {:?}: one of `ostrov plugin`'s own words", m.id));
    }
    if m.api != API {
        return Err(format!("api {}: this ostrov speaks {API}", m.api));
    }
    if m.exec.trim().is_empty() {
        return Err("no exec".into());
    }
    if let Some(l) = m.launcher.iter().find(|l| l.prefix.trim_start() != l.prefix || l.prefix.is_empty()) {
        return Err(format!("launcher prefix {:?}: empty, or starting with a space", l.prefix));
    }
    if let Some(k) = m.keys.iter().find(|k| !k.combo.contains(',') || k.command.trim().is_empty()) {
        return Err(format!("key {:?}: not \"MODS, KEY\", or no command", k.combo));
    }
    for w in &mut m.widgets {
        if !word(&w.id) {
            return Err(format!("widget id {:?}: not of [a-z0-9-]", w.id));
        }
        w.sizes.retain(|&(x, y)| (1..=crate::cc::grid::COLS).contains(&x) && y >= 1);
        if w.sizes.is_empty() {
            w.sizes.push((4, 1));
        }
        // a widget saying no icon of its own has its plugin's (its menu's head, its badge until drawn)
        if w.icon.is_empty() {
            w.icon = m.icon.clone();
        }
    }
    Ok(m)
}

/// A plugin's settings schema as it sent it (settings/'s Schema, as JSON): on the Settings page as plugin.<id>,
/// its fields in [plugin.<id>], its secrets "plugin.<id>.<key>"; kept for `ostrov plugins`. A button of its
/// (an action) runs the plugin's command `action ID` with the field's value as input, its outcome shown.
fn settings(p: &Plugin, json: Value) {
    let mut schema: crate::settings::Schema = match serde_json::from_value(json.clone()) {
        Ok(s) => s,
        Err(e) => return p.log(&format!("settings schema: {e}")),
    };
    for a in schema.sections.iter_mut().flat_map(|s| s.fields.iter_mut()).flat_map(|f| f.actions.iter_mut()) {
        let (b, id) = (p.backend.clone(), a.id.clone());
        a.run = Some(Rc::new(move |v: &Value, done: Rc<dyn Fn(Result<String, String>)>| {
            let input = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
            let r = b.run(&["action".to_string(), id.clone()], Some(input));
            glib::spawn_future_local(async move { done(r.await) });
        }));
    }
    crate::settings::register(&format!("plugin.{}", p.m.id), &p.m.name, &p.m.icon, schema);
    *p.settings.borrow_mut() = Some(json);
}

/// What draws widgets beside its Backend: the exports GTK's side calls, by whatever transport.
pub trait Draws: Backend {
    /// A widget's tree now; null keeps what is drawn.
    fn render(&self, widget: &str) -> BoxFut<Result<Value, String>>;
    fn on_event(&self, widget: &str, node: &str, event: &str, value: String);
    fn on_config(&self, config: Value);
    /// The desktop's state, to a plugin that may have it.
    fn on_state(&self, state: &Value);
    /// A launcher mode's hits for what is typed after its prefix, as JSON.
    fn query(&self, mode: &str, text: &str) -> BoxFut<Result<Value, String>>;
    /// One of its hits picked, what was typed beside it.
    fn pick(&self, mode: &str, id: &str, text: &str);
    /// One of ostrov's events (src/events.rs) it follows.
    fn on_shell_event(&self, name: &str, payload: &Value);
    /// Its calendar's events between two local ISO times, as JSON.
    fn calendar_events(&self, from: &str, to: &str) -> BoxFut<Result<Value, String>>;
}

/// What a plugin's transport hands GTK's thread: ostrov's commands it runs (their outcome back through the
/// sender), its settings' schema, and what it pushes unasked, a widget's tree or its state.
pub enum Up {
    Run(Vec<String>, async_channel::Sender<Result<String, String>>),
    /// a dialog asked (its JSON), its answer back, None for a no
    Ask(Value, async_channel::Sender<Option<String>>),
    Settings(Value),
    Render(String, Value),
    State(Value),
}

pub struct Plugin {
    m: Manifest,
    dir: PathBuf,
    backend: Arc<dyn Draws>,
    /// each widget's tree as last drawn, and the widgets made for the grid
    trees: RefCell<HashMap<String, node::El>>,
    /// each badge's tree as last drawn, and whether it said it is active
    badges: RefCell<HashMap<String, (node::El, bool)>>,
    views: RefCell<Vec<Weak<View>>>,
    config: RefCell<Value>,
    state: RefCell<Value>,
    settings: RefCell<Option<Value>>,
    /// a dialog of its asked and not answered yet
    asking: Cell<bool>,
    /// its worker on the plugins' runtime, stopped (its process killed) when it is removed
    task: Option<tokio::task::AbortHandle>,
    /// one of ostrov's own, from a system directory
    official: bool,
}

/// A widget drawn from node trees (a plugin's, a KDL file's) made for the grid: the box its tree is drawn into,
/// its menu's, its badge's, as last drawn; how to have its panel draw everything again.
pub struct View {
    pub wid: String,
    tile: gtk4::Box,
    items: gtk4::Box,
    flip: Rc<dyn Fn()>,
    pub again: Rc<dyn Fn()>,
    size: Cell<(u8, u8)>,
    drawn: RefCell<Option<node::Drawn>>,
    menu: RefCell<Option<node::Drawn>>,
    badge: Option<Badge>,
}

/// A view's badge: the box its tree is drawn into, the cell its panel reads it active by.
struct Badge {
    root: gtk4::Box,
    active: Rc<Cell<bool>>,
    drawn: RefCell<Option<node::Drawn>>,
}

impl View {
    /// One made for the grid, its name on it until it is painted; with a badge, the icon in it until then.
    pub fn make(wid: &str, name: &str, icon: &str, size: (u8, u8), badge: bool, c: &Ctx) -> (Rc<View>, Widget) {
        let tile = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        tile.add_css_class("plugin-card");
        tile.append(&crate::style::label(name, "dim"));
        let (card, items) = crate::ui::menu(icon, name);
        let badge = badge.then(|| {
            let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            root.append(&gtk4::Image::from_icon_name(icon));
            Badge { root, active: Rc::new(Cell::new(false)), drawn: RefCell::default() }
        });
        let face = badge.as_ref().map(|b| Face { root: b.root.clone().upcast(), active: b.active.clone() });
        let view = Rc::new(View {
            wid: wid.into(),
            tile: tile.clone(),
            items,
            flip: c.flip.clone(),
            again: c.again.clone(),
            size: Cell::new(size),
            drawn: RefCell::default(),
            menu: RefCell::default(),
            badge,
        });
        let v = view.clone();
        let w = Widget {
            size: Box::new(move |w, h| {
                v.size.set((w, h));
                v.drawn.borrow().iter().flat_map(|d| &d.toggles).for_each(|t| t.size(w, h));
            }),
            face,
            ..Widget::new(&tile, Some(&card), |_| ())
        };
        (view, w)
    }

    /// The tree in the tile, its root's menu in the menu's card.
    pub fn paint(&self, tree: &node::El, emit: &node::Emit) {
        node::draw(&self.tile, tree, &mut self.drawn.borrow_mut(), emit, Some(self.flip.clone()));
        // the kit's own ground for a toggle, a slider, a round button; a card's for anything else
        let own = matches!(tree.kind, node::Kind::Toggle { .. } | node::Kind::Slider { .. } | node::Kind::Round { .. });
        let classes: &[&str] = if own { &[] } else { &["plugin-card"] };
        self.tile.set_css_classes(classes);
        let (w, h) = self.size.get();
        self.drawn.borrow().iter().flat_map(|d| &d.toggles).for_each(|t| t.size(w, h));
        match tree.menu() {
            Some(m) => node::draw(&self.items, m, &mut self.menu.borrow_mut(), emit, None),
            None => {
                crate::style::clear(&self.items);
                *self.menu.borrow_mut() = None;
            }
        }
    }

    /// The badge's tree drawn, and whether it is active: whether that changed (its panel's face to be fitted).
    pub fn paint_badge(&self, tree: &node::El, active: bool, emit: &node::Emit) -> bool {
        let Some(b) = &self.badge else { return false };
        node::draw(&b.root, tree, &mut b.drawn.borrow_mut(), emit, None);
        b.active.replace(active) != active
    }
}

thread_local! {
    static PLUGINS: RefCell<Vec<Rc<Plugin>>> = const { RefCell::new(Vec::new()) };
    static MODES: RefCell<Vec<Rc<Mode>>> = const { RefCell::new(Vec::new()) };
}

/// A plugin's launcher mode, its prefix claimed.
pub struct Mode {
    pub decl: ModeDecl,
    /// its plugin's id
    id: String,
    backend: Arc<dyn Draws>,
}

impl Mode {
    /// Its hits for text, what is typed after the prefix; an error after 2 s, the launcher being typed into.
    pub async fn query(&self, text: &str) -> Result<Vec<ModeHit>, String> {
        let call = self.backend.query(&self.decl.prefix, text);
        let late = glib::timeout_future(std::time::Duration::from_secs(2));
        match futures_util::future::select(call, std::pin::pin!(late)).await {
            futures_util::future::Either::Left((r, _)) => parse_hits(r?),
            futures_util::future::Either::Right(_) => Err("no hits in 2 s".into()),
        }
    }

    pub fn pick(&self, id: &str, text: &str) {
        self.backend.pick(&self.decl.prefix, id, text);
    }
}

/// The plugin's mode whose prefix what is typed starts with, and what is typed after it.
pub fn mode(typed: &str) -> Option<(Rc<Mode>, &str)> {
    let m = MODES.with(|ms| ms.borrow().iter().find(|m| typed.starts_with(&m.decl.prefix)).cloned())?;
    let rest = &typed[m.decl.prefix.len()..];
    Some((m, rest))
}

/// Whether a prefix may be claimed besides those taken (the launcher's own first): not when either starts with
/// the other, as what is typed would go to one of them alone.
fn free(prefix: &str, taken: &[String]) -> bool {
    taken.iter().all(|t| !t.starts_with(prefix) && !prefix.starts_with(t.as_str()))
}

/// The user's plugins' directory, where `ostrov plugin install` puts them.
fn dir() -> PathBuf {
    crate::hub::home().join(".local/share/ostrov/plugins")
}

/// The official plugins' directories: share/ beside ostrov's executable's directory (/usr/bin/ostrov's
/// /usr/share, a Nix store path's), then XDG_DATA_DIRS's (/usr/local/share:/usr/share if unset).
fn system_dirs() -> Vec<PathBuf> {
    let own = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.parent()?.join("share")));
    let xdg = std::env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty());
    let xdg = xdg.unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    own.into_iter().chain(std::env::split_paths(&xdg)).map(|d| d.join("ostrov/plugins")).collect()
}

/// Every plugin found, sorted by id, and whether it is official: the user's, then the official ones whose ids
/// none before took.
fn found() -> Vec<(PathBuf, Manifest, bool)> {
    let mut all: Vec<(PathBuf, Manifest, bool)> = discover(&dir()).into_iter().map(|(d, m)| (d, m, false)).collect();
    for d in system_dirs() {
        for (d, m) in discover(&d) {
            if !all.iter().any(|(_, o, _)| o.id == m.id) {
                all.push((d, m, true));
            }
        }
    }
    all.sort_by(|a, b| a.1.id.cmp(&b.1.id));
    all
}

/// Whether a plugin runs: `[plugin.<id>] enabled`, else yes for one the user installed, no for an official one.
fn enabled(cfg: &crate::config::Config, id: &str, official: bool) -> bool {
    cfg.plugin.get(id).and_then(|t| t.get("enabled")).and_then(|v| v.as_bool()).unwrap_or(!official)
}

/// The config's [plugin.<id>], {} if none.
fn config(cfg: &crate::config::Config, id: &str) -> Value {
    cfg.plugin.get(id).and_then(|t| serde_json::to_value(t).ok()).unwrap_or_else(|| json!({}))
}

impl Plugin {
    fn log(&self, what: &str) {
        eprintln!("ostrov: plugin {}: {what}", self.m.id);
    }

    /// A widget's tree, rendered or pushed: kept, and drawn into its views.
    /// A badge's tree ("<wid>#bar") the same, its top-level "active" kept beside it.
    fn set_tree(self: &Rc<Self>, wid: &str, tree: &Value) {
        if tree.is_null() {
            return;
        }
        let (base, bar) = match wid.strip_suffix("#bar") {
            Some(b) => (b, true),
            None => (wid, false),
        };
        if !self.m.widgets.iter().any(|w| w.id == base && (w.badge || !bar)) {
            return self.log(&format!("render: no widget {wid:?} in the manifest"));
        }
        match node::El::deserialize(tree) {
            Ok(el) if bar => {
                let active = tree["active"].as_bool().unwrap_or(false);
                self.badges.borrow_mut().insert(base.to_string(), (el, active));
            }
            Ok(el) => {
                self.trees.borrow_mut().insert(base.to_string(), el);
            }
            Err(e) => return self.log(&format!("render {wid}: {e}")),
        }
        self.views.borrow_mut().retain(|w| w.upgrade().is_some());
        let views: Vec<Rc<View>> = self.views.borrow().iter().filter_map(Weak::upgrade).collect();
        for view in views.iter().filter(|v| v.wid == base) {
            self.paint(view);
        }
    }

    /// A view drawn from its widget's last trees, its badge's too; its panel's face fitted if the badge's
    /// activity changed.
    fn paint(&self, view: &View) {
        let (b, wid) = (self.backend.clone(), view.wid.clone());
        let emit: node::Emit = Rc::new(move |node, event, value| b.on_event(&wid, node, event, value));
        if let Some(tree) = self.trees.borrow().get(&view.wid).cloned() {
            view.paint(&tree, &emit);
        }
        let badge = self.badges.borrow().get(&view.wid).cloned();
        if let Some((tree, active)) = badge
            && view.paint_badge(&tree, active, &emit)
        {
            (view.again)();
        }
    }

    /// One of its widgets made for the grid, as last drawn (its name until the plugin draws it).
    fn widget(&self, decl: &WidgetDecl, c: &Ctx) -> Widget {
        let (view, w) = View::make(&decl.id, &decl.name, &decl.icon, decl.sizes[0], decl.badge, c);
        self.views.borrow_mut().push(Rc::downgrade(&view));
        self.paint(&view);
        w
    }

    /// A dialog it asks, its icon and id above it all, so it passes for nothing else; one at a time, a no after
    /// five minutes unanswered.
    fn ask(self: &Rc<Self>, spec: Value, reply: async_channel::Sender<Option<String>>) {
        if self.asking.replace(true) {
            self.log("ask: one dialog at a time");
            let _ = reply.try_send(None);
            return;
        }
        let me = self.clone();
        glib::spawn_future_local(async move {
            let from = (me.m.icon.clone(), me.m.id.clone());
            let r = crate::prompt::dialog(&spec, Some(from), Some(std::time::Duration::from_secs(300))).await;
            me.asking.set(false);
            let _ = reply.send(r.unwrap_or_else(|e| {
                me.log(&format!("ask: {e}"));
                None
            })).await;
        });
    }

    /// Its kicks (its state and every widget's tree pulled anew, a burst of kicks once) and what its transport
    /// hands up, on GTK's thread.
    fn listen(self: Rc<Self>, kicks: async_channel::Receiver<()>, ups: async_channel::Receiver<Up>) {
        let me = self.clone();
        glib::spawn_future_local(async move {
            while kicks.recv().await.is_ok() {
                while kicks.try_recv().is_ok() {}
                let st = me.backend.state().await;
                *me.state.borrow_mut() = st;
                for w in me.m.widgets.iter().map(|w| w.id.clone()).collect::<Vec<_>>() {
                    match me.backend.render(&w).await {
                        Ok(tree) => me.set_tree(&w, &tree),
                        Err(e) => me.log(&format!("render {w}: {e}")),
                    }
                }
                // the badges, as render("<wid>#bar")
                for w in me.m.widgets.iter().filter(|w| w.badge).map(|w| format!("{}#bar", w.id)).collect::<Vec<_>>() {
                    match me.backend.render(&w).await {
                        Ok(tree) => me.set_tree(&w, &tree),
                        Err(e) => me.log(&format!("render {w}: {e}")),
                    }
                }
            }
        });
        glib::spawn_future_local(async move {
            while let Ok(up) = ups.recv().await {
                match up {
                    Up::Run(args, reply) => {
                        let r = crate::command(&args, None);
                        glib::spawn_future_local(async move {
                            let _ = reply.send(r.await).await;
                        });
                    }
                    Up::Settings(schema) => settings(&self, schema),
                    Up::Ask(spec, reply) => self.ask(spec, reply),
                    Up::Render(w, tree) => self.set_tree(&w, &tree),
                    Up::State(st) => *self.state.borrow_mut() = st,
                }
            }
        });
    }
}

/// A manifest's texts (its name, description, widgets' and launcher modes' names, commands' help) in the user's
/// language, by the plugin's own catalogue, i18n/<lang>.toml in its directory ("English" = "theirs", as ostrov's
/// own and as the plugin's texts are looked up by the SDK's t()).
fn translate(m: &mut Manifest, dir: &Path) {
    let lang = crate::i18n::lang();
    let Ok(text) = std::fs::read_to_string(dir.join(format!("i18n/{lang}.toml"))) else { return };
    let words: HashMap<String, String> = match toml::from_str(&text) {
        Ok(w) => w,
        Err(e) => return eprintln!("ostrov: plugin {}: i18n/{lang}.toml: {e}", m.id),
    };
    let t = |s: &mut String| {
        if let Some(w) = words.get(s.as_str()) {
            *s = w.clone();
        }
    };
    t(&mut m.name);
    t(&mut m.description);
    m.widgets.iter_mut().for_each(|w| t(&mut w.name));
    m.launcher.iter_mut().for_each(|l| t(&mut l.name));
    m.commands.iter_mut().for_each(|c| t(&mut c.help));
}

/// The plugins found in dir, sorted by id; one that does not read said and passed over.
fn discover(dir: &Path) -> Vec<(PathBuf, Manifest)> {
    let mut found: Vec<(PathBuf, Manifest)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let text = std::fs::read_to_string(path.join("manifest.toml")).ok()?;
            let name = e.file_name().to_string_lossy().into_owned();
            let m = parse_manifest(&text, &name).map_err(|err| eprintln!("ostrov: plugin {}: {err}", path.display()));
            m.ok().map(|mut m| {
                translate(&mut m, &path);
                (path, m)
            })
        })
        .collect();
    found.sort_by(|a, b| a.1.id.cmp(&b.1.id));
    found
}

/// The plugins' Tokio runtime, a thread of its own, made as the first plugin starts.
fn rt() -> Option<&'static tokio::runtime::Runtime> {
    static RT: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();
    RT.get_or_init(|| {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build();
        rt.map_err(|e| eprintln!("ostrov: plugins: {e}")).ok()
    })
    .as_ref()
}

/// A plugin started: its worker on the plugins' runtime, its kicks and messages listened to, its launcher modes
/// claimed where their prefixes are free, its calendar a source of the calendar's.
fn load(dir: PathBuf, m: Manifest, official: bool, cfg: &crate::config::Config) -> Rc<Plugin> {
    let (up, ups) = async_channel::unbounded();
    let (kick, kicks) = async_channel::unbounded();
    let backend = Arc::new(process::Process::new(&m, dir.clone(), config(cfg, &m.id), up));
    let task = rt().map(|rt| rt.spawn(backend.worker(kick)).abort_handle());
    let p = Rc::new(Plugin {
        config: RefCell::new(config(cfg, &m.id)),
        m,
        dir,
        backend,
        trees: RefCell::default(),
        badges: RefCell::default(),
        views: RefCell::default(),
        state: RefCell::default(),
        settings: RefCell::default(),
        asking: Cell::new(false),
        task,
        official,
    });
    p.clone().listen(kicks, ups);
    MODES.with(|ms| {
        let mut ms = ms.borrow_mut();
        let mut taken: Vec<String> = crate::launcher::PREFIXES.iter().map(|p| p.to_string()).collect();
        taken.extend(ms.iter().map(|m| m.decl.prefix.clone()));
        for decl in &p.m.launcher {
            if !free(&decl.prefix, &taken) {
                p.log(&format!("launcher prefix {:?}: taken", decl.prefix));
                continue;
            }
            taken.push(decl.prefix.clone());
            ms.push(Rc::new(Mode { decl: decl.clone(), id: p.m.id.clone(), backend: p.backend.clone() }));
        }
    });
    if p.m.calendar && p.m.allows("calendar") {
        let b = p.backend.clone();
        let source: crate::modules::calendar::service::Source = Arc::new(move |from, to| b.calendar_events(&from, &to));
        crate::modules::calendar::service::source(&p.m.id, Some(source));
    }
    p
}

/// A plugin started while ostrov runs, among the others by id, its keys bound (its widgets join the gallery at
/// the next restart, the registry being made once).
fn add(dir: PathBuf, m: Manifest, official: bool, cfg: &crate::config::Config) {
    let p = load(dir, m, official, cfg);
    PLUGINS.with(|ps| {
        let mut ps = ps.borrow_mut();
        ps.push(p);
        ps.sort_by(|a, b| a.m.id.cmp(&b.m.id));
    });
    bind_keys(None);
}

/// A running plugin stopped: its process ended, its launcher modes, keys and calendar gone. Whether it had
/// widgets or settings, which leave at the next restart; None if it was not running.
fn stop(id: &str) -> Option<bool> {
    let p = PLUGINS.with(|ps| {
        let mut ps = ps.borrow_mut();
        let i = ps.iter().position(|p| p.m.id == id)?;
        Some(ps.remove(i))
    })?;
    if let Some(t) = &p.task {
        t.abort();
    }
    MODES.with(|ms| ms.borrow_mut().retain(|m| m.id != id));
    crate::modules::calendar::service::source(id, None);
    bind_keys(Some(&p.m));
    Some(!p.m.widgets.is_empty() || p.settings.borrow().is_some())
}

/// The plugins turned on or off by their `enabled` since they were started, started or stopped.
fn follow_enabled(cfg: &crate::config::Config) {
    for (dir, m, official) in found() {
        let running = PLUGINS.with(|ps| ps.borrow().iter().any(|p| p.m.id == m.id));
        match (enabled(cfg, &m.id, official), running) {
            (true, false) => add(dir, m, official, cfg),
            (false, true) => {
                stop(&m.id);
            }
            _ => {}
        }
    }
}

/// Every plugin's keys handed to hyprland.rs, bound where free; those of a plugin gone unbound.
fn bind_keys(gone: Option<&Manifest>) {
    let all = PLUGINS.with(|ps| ps.borrow().iter().flat_map(|p| p.m.hypr_keys()).collect());
    crate::modules::hyprland::plugin_keys(all, gone.map(Manifest::hypr_keys).unwrap_or_default());
}

/// Every plugin found started; the desktop's state sent on to those that may have it, ostrov's events to those
/// following them, the config to each whose section changed; their keys bound. Before the control centre is
/// built, which takes their widgets (metas).
pub fn start(hub: &Rc<Hub>) {
    let cfg = crate::config::load();
    let on = found().into_iter().filter(|(_, m, official)| enabled(&cfg, &m.id, *official));
    let plugins: Vec<Rc<Plugin>> = on.map(|(dir, m, official)| load(dir, m, official, &cfg)).collect();
    PLUGINS.with(|ps| *ps.borrow_mut() = plugins);
    bind_keys(None);
    hub.on(|st| {
        if !st.is_null() {
            PLUGINS.with(|ps| ps.borrow().iter().for_each(|p| p.backend.on_state(st)));
        }
    });
    crate::events::on(|name, payload| {
        PLUGINS.with(|ps| {
            ps.borrow().iter().filter(|p| p.m.follows(name)).for_each(|p| p.backend.on_shell_event(name, payload))
        });
    });
    let file = gio::File::for_path(crate::config::path());
    if let Ok(mon) = file.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
        mon.connect_changed(|_, _, _, _| {
            let cfg = crate::config::load();
            follow_enabled(&cfg);
            PLUGINS.with(|ps| {
                for p in ps.borrow().iter() {
                    let now = config(&cfg, &p.m.id);
                    if *p.config.borrow() != now {
                        p.backend.on_config(now.clone());
                        *p.config.borrow_mut() = now;
                    }
                }
            })
        });
        // kept for the program's life
        std::mem::forget(mon);
    }
}

/// The plugins' widgets for the control centre's registry, plugin.<id>.<widget>.
pub fn metas() -> Vec<Meta> {
    // ponytail: the registry's names are 'static, so these are leaked, once, as the control centre is built;
    // were plugins ever loaded anew while ostrov runs, Meta would take Strings instead
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    PLUGINS.with(|ps| {
        let mut all = Vec::new();
        for p in ps.borrow().iter() {
            for (n, w) in p.m.widgets.iter().enumerate() {
                let p2 = p.clone();
                all.push(Meta {
                    id: leak(format!("plugin.{}.{}", p.m.id, w.id)),
                    name: leak(w.name.clone()),
                    icon: leak(if w.icon.is_empty() { p.m.icon.clone() } else { w.icon.clone() }),
                    sizes: Box::leak(w.sizes.clone().into_boxed_slice()),
                    make: Rc::new(move |c| p2.widget(&p2.m.widgets[n], c)),
                    settings: None,
                    bar: w.bar,
                });
            }
        }
        all
    })
}

/// `ostrov plugins`: each plugin running as its manifest has it, its state, its settings' schema; then those
/// found but off, `"enabled": false`.
pub fn list() -> String {
    let mut all: Vec<Value> = PLUGINS.with(|ps| {
        ps.borrow()
            .iter()
            .map(|p| {
                let m = &p.m;
                json!({
                    "id": m.id, "name": m.name, "version": m.version, "description": m.description,
                    "dir": p.dir, "official": p.official, "enabled": true,
                    "permissions": m.permissions, "state": *p.state.borrow(),
                    "widgets": m.widgets.iter().map(|w| format!("plugin.{}.{}", m.id, w.id)).collect::<Vec<_>>(),
                    "settings": *p.settings.borrow(),
                    "launcher": m.launcher.iter().map(|l| &l.prefix).collect::<Vec<_>>(),
                    "events": m.events, "calendar": m.calendar,
                    "keys": m.keys.iter().map(|k| json!({"combo": k.combo, "command": k.command})).collect::<Vec<_>>(),
                })
            })
            .collect()
    });
    for (dir, m, official) in found() {
        if !PLUGINS.with(|ps| ps.borrow().iter().any(|p| p.m.id == m.id)) {
            all.push(json!({
                "id": m.id, "name": m.name, "version": m.version, "description": m.description,
                "dir": dir, "official": official, "enabled": false, "permissions": m.permissions,
            }));
        }
    }
    all.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    serde_json::to_string_pretty(&all).unwrap_or_default()
}

/// A plugin's commands as its manifest has them.
fn usage(m: &Manifest) -> String {
    let mut s = format!("{} ({}): {}", m.id, m.name, m.description);
    for c in &m.commands {
        let words = format!("ostrov plugin {} {}", m.id, c.form());
        s += &format!("\n  {words:<40} {}", c.help);
    }
    s
}

impl CommandDecl {
    /// Its words after the plugin's id.
    fn form(&self) -> &str {
        if self.usage.is_empty() { &self.name } else { &self.usage }
    }
}

/// The plugins for completion: their ids, names, commands' forms with their help (the form itself if none).
pub fn known() -> Vec<crate::forms::Plugin> {
    PLUGINS.with(|ps| {
        let ps = ps.borrow();
        let commands = |m: &Manifest| -> Vec<(String, String)> {
            let help = |c: &CommandDecl| if c.help.is_empty() { c.form().to_string() } else { c.help.clone() };
            m.commands.iter().map(|c| (c.form().to_string(), help(c))).collect()
        };
        let known = |m: &Manifest| {
            crate::forms::Plugin { id: m.id.clone(), name: m.name.clone(), commands: commands(m) }
        };
        ps.iter().map(|p| known(&p.m)).collect()
    })
}

/// The plugins' widgets, plugin.<id>.<widget>, by name.
pub fn widgets() -> Vec<(String, String)> {
    PLUGINS.with(|ps| {
        let ps = ps.borrow();
        let each = |p: &Rc<Plugin>| {
            let m = &p.m;
            m.widgets.iter().map(|w| (format!("plugin.{}.{}", m.id, w.id), w.name.clone())).collect::<Vec<_>>()
        };
        ps.iter().flat_map(each).collect()
    })
}

/// Every plugin's commands, under it.
pub fn help() -> String {
    let all: Vec<String> = PLUGINS.with(|ps| ps.borrow().iter().map(|p| usage(&p.m)).collect());
    if all.is_empty() { "no plugins".into() } else { all.join("\n") }
}

/// `ostrov plugin ID ARGS`: the plugin's own command (its export run), input what was piped to ostrov; with no
/// ARGS or help its commands, with no ID every plugin's. `install SOURCE` and `remove ID` install.rs's.
pub fn run(args: &[String], input: Option<String>) -> crate::Reply {
    let ready = |r| -> crate::Reply { Box::pin(std::future::ready(r)) };
    match args {
        [w, src] if w == "install" => return Box::pin(install::install(src.clone())),
        [w, id] if w == "remove" => return ready(install::remove(id)),
        [w, ..] if RESERVED.contains(&w.as_str()) => {
            return ready(Err("ostrov plugin install SOURCE | remove ID".into()));
        }
        _ => {}
    }
    let Some((id, rest)) = args.split_first() else { return ready(Ok(help())) };
    let Some(p) = PLUGINS.with(|ps| ps.borrow().iter().find(|p| p.m.id == *id).cloned()) else {
        return ready(Err(format!("no plugin {id}; the plugins:\n{}", help())));
    };
    if rest.is_empty() || rest == ["help"] {
        return ready(Ok(usage(&p.m)));
    }
    p.backend.run(rest, input)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = r#"
        id = "hello"
        name = "Hello"
        version = "0.1.0"
        api = 1
        exec = "python3 main.py"
        icon = "face-smile-symbolic"
        permissions = ["state", "secrets", "events", "keys"]
        homepage = "https://example.org"   # unknown: ignored
        events = ["window", "power"]
        calendar = true

        [[keys]]
        combo = "SUPER, F12"
        command = " toggle "

        [[widgets]]
        id = "counter"
        name = "Counter"
        sizes = [[4, 1], [2, 1], [9, 1], [0, 1]]
        badge = true
        bar = "active"

        [[widgets]]
        id = "bare"
        name = "Bare"

        [[commands]]
        name = "set"
        usage = "set N"
        help = "the count set"

        [[commands]]
        name = "count"

        [[launcher]]
        prefix = "?"
        name = "Ask"
        icon = "help-symbolic"
        later = true                       # unknown: ignored
    "#;

    #[test]
    fn manifest_reads() {
        let m = parse_manifest(HELLO, "hello").unwrap();
        assert_eq!((m.id.as_str(), m.api, m.exec.as_str()), ("hello", 1, "python3 main.py"));
        assert_eq!(m.permissions, ["state", "secrets", "events", "keys"]);
        assert!(m.calendar && !m.allows("calendar"));
        assert!(m.follows("window") && m.follows("power") && !m.follows("lock"));
        assert_eq!(m.hypr_keys(), [("hello".to_string(), "SUPER, F12".to_string(), "toggle".to_string())]);
        // what the permissions do not give is not had
        let bare = parse_manifest(&HELLO.replace(", \"events\", \"keys\"", ""), "hello").unwrap();
        assert!(!bare.follows("window") && bare.hypr_keys().is_empty());
        let all = parse_manifest(&HELLO.replace("[\"window\", \"power\"]", "[\"*\"]"), "hello").unwrap();
        assert!(all.follows("lock"));
        assert!(parse_manifest(&HELLO.replace("\"SUPER, F12\"", "\"F12\""), "hello").is_err());
        assert!(parse_manifest(&HELLO.replace("\" toggle \"", "\" \""), "hello").is_err());
        assert_eq!(m.widgets[0].sizes, [(4, 1), (2, 1)]);
        assert_eq!(m.widgets[1].sizes, [(4, 1)]);
        assert_eq!(m.widgets[1].icon, m.icon, "the plugin's when it says none");
        assert_eq!((m.widgets[0].badge, m.widgets[0].bar), (true, Show::Active));
        assert_eq!((m.widgets[1].badge, m.widgets[1].bar), (false, Show::Never));
        let u = usage(&m);
        assert!(u.contains("ostrov plugin hello set N") && u.contains("the count set"), "{u}");
        assert!(u.contains("ostrov plugin hello count"), "{u}");
        assert_eq!(m.launcher, [ModeDecl { prefix: "?".into(), name: "Ask".into(), icon: "help-symbolic".into() }]);
        assert!(parse_manifest(&HELLO.replace("prefix = \"?\"", "prefix = \"\""), "hello").is_err());
        assert!(parse_manifest(&HELLO.replace("prefix = \"?\"", "prefix = \" a\""), "hello").is_err());
        let bare = parse_manifest("id = \"x\"\nname = \"X\"\napi = 1\nexec = \"x\"", "x").unwrap();
        assert!(bare.launcher.is_empty());
        assert!(bare.events.is_empty() && bare.keys.is_empty() && !bare.calendar);
        assert!(parse_manifest("id = \"install\"\nname = \"X\"\napi = 1\nexec = \"x\"", "install").is_err());
    }

    #[test]
    fn hits_read() {
        let v = serde_json::json!([
            {"id": "1", "text": "Ask", "note": "web", "icon": "i", "open": "https://x", "extra": 1},
            {"text": "Copied", "copy": "c"},
            {"id": "no text"},
        ]);
        let hits = parse_hits(v).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!((hits[0].id.as_str(), hits[0].open.as_deref()), ("1", Some("https://x")));
        assert_eq!(hits[0].copy, None);
        assert_eq!((hits[1].text.as_str(), hits[1].copy.as_deref(), hits[1].note.as_str()), ("Copied", Some("c"), ""));
        assert_eq!(parse_hits(Value::Null).unwrap(), []);
        assert!(parse_hits(serde_json::json!({"text": "x"})).is_err());
    }

    #[test]
    fn prefixes_conflict() {
        let taken: Vec<String> = crate::launcher::PREFIXES.iter().map(|p| p.to_string()).collect();
        assert!(free("?", &taken));
        assert!(free("g", &taken));
        assert!(free("sh", &taken));
        assert!(!free(":", &taken));
        assert!(!free("s", &taken));
        assert!(!free(":x", &taken));
        assert!(!free("/", &taken));
        let taken = [taken, vec!["?".into()]].concat();
        assert!(!free("??", &taken));
    }

    #[test]
    fn manifest_checked() {
        assert!(parse_manifest(HELLO, "other").unwrap_err().contains("directory"));
        assert!(parse_manifest(&HELLO.replace("api = 1", "api = 2"), "hello").unwrap_err().contains("api 2"));
        assert!(parse_manifest(&HELLO.replace("exec = \"python3 main.py\"", "exec = \" \""), "hello").is_err());
        assert!(parse_manifest(&HELLO.replace("\"counter\"", "\"a.b\""), "hello").is_err());
        assert!(parse_manifest(&HELLO.replace("\"hello\"", "\"Hello\""), "Hello").is_err());
        assert!(parse_manifest(&HELLO.replace("\"hello\"", "\"he_llo\""), "he_llo").is_err());
        assert!(parse_manifest("id = \"x\"\nname = \"X\"\napi = 1", "x").is_err());
    }
}
