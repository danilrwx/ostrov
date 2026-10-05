//! The panels: what unrolls out of the bar, each a grid of widgets 8 cells wide (grid.rs), its face in the bar
//! made of its widgets' badges. The control centre (control) and the calendar (calendar) are two; [panels.ID] in
//! the config makes more. Each widget is one of the modules' (modules/) or the plugins', in one of the sizes it
//! allows: a toggle from a square to the whole width, a slider the whole width alone. Its menu, if it has one,
//! unfolds under its rows across the whole width, one menu at a time, sliding open in 100 ms. A widget may put a
//! badge (an icon, a few words) in its panel's face in the bar: always, only while it is active (a VPN up, an
//! event near), or never; the panel's own icon there when none does.
//!
//! "Edit" turns the grid to its editing: a widget dragged goes where it is dropped, the ones in its way pushed
//! down; dragged by its corner it takes the size nearest the pointer of those it allows; its minus takes it off;
//! a click picks it, its inspector under the grid: its sizes, when its badge shows in the bar (always, while
//! active, never), the bar alone (its tile off the panel), its settings, off; the gallery under it puts back any
//! widget not on it. The layouts are
//! kept in ~/.config/ostrov/panel.toml, a list of widgets a panel.
//!
//! Under the control centre's grid, beside Edit, a gear and a palette: the Settings page (settings/: a form for
//! each of ostrov's sections, each widget with a schema, each plugin's) and the Appearance page (appearance.rs),
//! each sliding in over the grid in the same popup, its arrow back at its top. The rows are as tall as
//! [appearance]'s density.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Align, Orientation};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::hub::Hub;
use crate::i18n::t;
use crate::popup::{Popup, Side};
use crate::style::{clear, label};

pub mod appearance;
pub mod grid;

use grid::{Item, BASE};

/// A widget made: its tile, its menu's card, its badge for the bar, how it draws the state and fits a size.
pub struct Widget {
    pub root: gtk4::Widget,
    pub menu: Option<gtk4::Widget>,
    pub face: Option<Face>,
    pub draw: Box<dyn Fn(&Value)>,
    pub size: Box<dyn Fn(u8, u8)>,
}

/// A widget's badge in its panel's face in the bar, kept up by the widget as it draws; active while what it
/// shows is on (a VPN up), for a badge shown only then.
pub struct Face {
    pub root: gtk4::Widget,
    pub active: Rc<Cell<bool>>,
    /// what a click on it does where it stands in the bar by itself (widget.ID), if not its menu: a plugin's
    /// badge's "click"
    pub click: Click,
}

/// A badge's own click, set and unset as it is drawn.
pub type Click = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

impl Face {
    pub fn new(root: &impl IsA<gtk4::Widget>) -> Face {
        Face { root: root.clone().upcast(), active: Rc::new(Cell::new(true)), click: Click::default() }
    }

    /// A badge of an icon alone, and the cell its widget says it is active by.
    pub fn icon(icon: &str) -> (Face, gtk4::Image) {
        let img = gtk4::Image::from_icon_name(icon);
        (Face::new(&img), img)
    }
}

/// When a widget's badge is in its panel's face.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Show {
    Always,
    Active,
    Never,
}

impl Show {
    const ALL: [(Show, &'static str); 3] = [(Show::Always, "Always"), (Show::Active, "While active"), (Show::Never, "Never")];
}

impl Widget {
    /// A widget with nothing to fit to its size.
    pub fn new(root: &impl IsA<gtk4::Widget>, menu: Option<&gtk4::Box>, draw: impl Fn(&Value) + 'static) -> Widget {
        Widget {
            root: root.clone().upcast(),
            menu: menu.map(|m| m.clone().upcast()),
            face: None,
            draw: Box::new(draw),
            size: Box::new(|_, _| ()),
        }
    }

    /// A toggle's widget: drawn by draw, fitted as the toggle fits, its badge the toggle's.
    pub fn toggle(t: &crate::ui::Toggle, menu: Option<&gtk4::Box>, draw: impl Fn(&Value) + 'static) -> Widget {
        let t2 = t.clone();
        Widget {
            size: Box::new(move |w, h| t2.size(w, h)),
            face: Some(Face { root: t.face.clone().upcast(), active: t.active.clone(), click: Click::default() }),
            ..Widget::new(&t.root, menu, draw)
        }
    }
}

/// What a widget is made with: the panel closed (a button that goes on elsewhere), its menu folded or unfolded,
/// every widget drawn again from the last state (something of its own changed: a passphrase asked for), the
/// last state itself.
pub struct Ctx {
    pub close: Rc<dyn Fn()>,
    pub flip: Rc<dyn Fn()>,
    pub again: Rc<dyn Fn()>,
    pub state: Rc<RefCell<Value>>,
}

/// A widget of the registry: its id, how the gallery shows it, the sizes it allows (the first its default).
pub struct Meta {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub sizes: &'static [(u8, u8)],
    pub make: Rc<dyn Fn(&Ctx) -> Widget>,
    /// its settings' schema, kept in [widget.ID], a page of the Settings
    pub settings: Option<fn() -> crate::settings::Schema>,
    /// when its badge is in the bar unless its panel says otherwise
    pub bar: Show,
}

/// The widgets there are: the modules', the plugins', the user's KDL files'. Made once, every panel's gallery the same.
pub fn registry() -> Rc<Vec<Meta>> {
    thread_local!(static REG: std::cell::OnceCell<Rc<Vec<Meta>>> = const { std::cell::OnceCell::new() });
    REG.with(|r| {
        r.get_or_init(|| {
            let mut reg: Vec<Meta> = crate::modules::ALL
                .iter()
                .flat_map(|m| m.widgets)
                .map(|d| Meta {
                    id: d.id,
                    name: t(d.name),
                    icon: d.icon,
                    sizes: d.sizes,
                    make: Rc::new(d.make),
                    settings: d.settings,
                    bar: d.bar,
                })
                .collect();
            reg.extend(crate::plugins::metas());
            let taken: Vec<&str> = reg.iter().map(|m| m.id).collect();
            reg.extend(crate::widgets::metas(&taken));
            for m in &reg {
                if let Some(schema) = m.settings {
                    crate::settings::register(&format!("widget.{}", m.id), m.name, m.icon, schema());
                }
            }
            Rc::new(reg)
        })
        .clone()
    })
}

/// The old panel's names for widgets' menus, `ostrov menu NAME` still takes them.
const ALIASES: &[(&str, &str)] = &[
    ("system", "session"),
    ("outs", "volume"),
    ("ins", "mic"),
    ("theme", "wallpaper"),
];

/// What `ostrov menu NAME` takes, for completion: the widgets (the plugins' too) by name, the old names, "edit".
pub fn menus() -> Vec<(String, String)> {
    let own = crate::modules::ALL.iter().flat_map(|m| m.widgets).map(|d| (d.id.to_string(), d.name.to_string()));
    let mut all: Vec<_> = own.chain(crate::plugins::widgets()).collect();
    all.extend(ALIASES.iter().map(|(old, key)| (old.to_string(), format!("{key}'s"))));
    all.push(("edit".into(), "the grid, edited".into()));
    all
}

/// A panel: its id, the name and icon its face falls back to, how wide it is, its layout until one is saved;
/// the control centre's has the Settings and Appearance pages under it.
pub struct Spec {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub width: i32,
    pub pages: bool,
    pub layout: Vec<Item>,
    /// how many cells wide its grid is, and its layout laid out for that; a cell's width in pixels
    pub cols: u8,
    pub cell: f64,
    /// [panels.ID] width set: the panel that wide whatever its cells
    fixed: bool,
}

impl Spec {
    /// The panel id: the control centre, the calendar, or one of [panels.ID] (empty until edited).
    pub fn of(id: &str) -> Spec {
        let it = |key: &str, x, y, w, h| Item { key: key.into(), x, y, w, h };
        // a cell's width, the panel as many of them wide as its grid has cells
        let spec = |name: &str, icon: &str, cell: f64, cols: u8, pages, layout| Spec {
            id: id.into(),
            name: name.into(),
            icon: icon.into(),
            width: (cell * cols as f64).round() as i32,
            pages,
            layout,
            cols,
            cell,
            fixed: false,
        };
        let cfg = crate::config::load().panels.remove(id);
        match id {
            // six cells wide: halves of three, the battery's and its three buttons' row
            "control" => spec(t("Control Centre"), "emblem-system-symbolic", 48.75, 6, true, vec![
                it("battery", 0, 0, 3, 1),
                it("screenshot", 3, 0, 1, 1),
                it("lock", 4, 0, 1, 1),
                it("session", 5, 0, 1, 1),
                it("volume", 0, 1, 6, 1),
                it("mic", 0, 2, 6, 1),
                it("brightness", 0, 3, 6, 1),
                it("wifi", 0, 4, 3, 1),
                it("bt", 3, 4, 3, 1),
                it("power", 0, 5, 3, 1),
                it("wallpaper", 3, 5, 3, 1),
                it("awake", 0, 6, 3, 1),
                it("headset", 3, 6, 3, 1),
                it("airplane", 0, 7, 3, 1),
            ]),
            // the calendar as it was: the weather and the player over the notifications at the left; the
            // date, the month, the coming events at the right
            "calendar" => spec(t("Calendar"), "x-office-calendar-symbolic", 85.0, 8, false, vec![
                it("weather", 0, 0, 4, 2),
                it("clock", 4, 0, 4, 1),
                it("month", 4, 1, 4, 5),
                it("media", 0, 2, 4, 2),
                it("notifications", 0, 4, 4, 6),
                it("agenda", 4, 6, 4, 4),
            ]),
            _ => spec(id, "view-grid-symbolic", 48.75, 6, false, Vec::new()),
        }
        .with(cfg)
    }

    fn with(mut self, cfg: Option<crate::config::PanelSpec>) -> Spec {
        if let Some(c) = cfg {
            self.name = c.name.unwrap_or(self.name);
            self.icon = c.icon.unwrap_or(self.icon);
            if let Some(w) = c.width {
                self.width = w;
                self.fixed = true;
            }
            self.cols = c.cols.map_or(self.cols, |n| n.clamp(MIN_COLS, MAX_COLS));
        }
        self
    }

    /// Its width for a grid that many cells wide: a set width as it is, else that many cells. A cell is an eighth
    /// of eight cells' width, the panel's padding (less a gap) in it: that taken out, each cell as wide whatever
    /// the cells (four cells not narrower each than eight, a round button not cut).
    fn width_for(&self, cols: u8) -> i32 {
        const EDGE: f64 = 22.0;
        if self.fixed { self.width } else { ((self.cell - EDGE / 8.0) * cols as f64 + EDGE).round() as i32 }
    }
}

/// How narrow and how wide a panel's grid goes, in cells.
const MIN_COLS: u8 = 4;
const MAX_COLS: u8 = 12;

/// A panel's cells across: [panels.ID] cols, else 8 for a layout saved before panels had a width of their own,
/// else its spec's.
fn cols_of(spec: &Spec) -> u8 {
    let set = crate::config::load().panels.get(&spec.id).and_then(|c| c.cols);
    let legacy = read_saved().is_some_and(|s| s.contains_key(&spec.id) || (spec.id == "control" && s.contains_key("widget")));
    match set {
        Some(n) => n.clamp(MIN_COLS, MAX_COLS),
        None if legacy => BASE,
        None => spec.cols,
    }
}

/// A widget placed on a grid, as panel.toml keeps it; bar, when its badge shows, if not as its widget says.
#[derive(Serialize, Deserialize)]
struct Placed {
    id: String,
    x: u8,
    y: u8,
    w: u8,
    h: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bar: Option<Show>,
    /// in the bar alone, no tile on the panel (the clock's time up there, nothing of it below)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    hidden: bool,
    /// across its tile: left, center, right; unset, filling it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    align: Option<String>,
    /// on the panel only while its widget is active (a drive plugged in); unset, always
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    while_active: bool,
}

/// How a widget lines up across its tile, as panel.toml names it, and its words in the inspector.
const ALIGNS: [(&str, Align, &str); 4] =
    [("fill", Align::Fill, "Fill"), ("left", Align::Start, "Left"), ("center", Align::Center, "Centre"), ("right", Align::End, "Right")];

fn align(name: &str) -> Align {
    ALIGNS.iter().find(|a| a.0 == name).map_or(Align::Fill, |a| a.1)
}

/// A panel's widgets' alignments as panel.toml has them.
fn aligns_of(id: &str) -> HashMap<String, String> {
    let saved = read_saved().unwrap_or_default();
    let placed = saved.get(id).into_iter().flatten();
    placed.filter_map(|p| Some((p.id.clone(), p.align.clone()?))).collect()
}

/// A panel's widgets on it only while active, as panel.toml has them.
fn while_active_of(id: &str) -> std::collections::HashSet<String> {
    let saved = read_saved().unwrap_or_default();
    saved.get(id).into_iter().flatten().filter(|p| p.while_active).map(|p| p.id.clone()).collect()
}

/// panel.toml: each panel's widgets, under its id ([[control]], [[calendar]]); [[widget]] the control centre's
/// from before there were panels.
type Saved = std::collections::BTreeMap<String, Vec<Placed>>;

fn read_saved() -> Option<Saved> {
    let t = std::fs::read_to_string(layout_path()).ok()?;
    toml::from_str(&t).map_err(|e| eprintln!("ostrov: {}: {e}", layout_path().display())).ok()
}

fn layout_path() -> std::path::PathBuf {
    crate::hub::home().join(".config/ostrov/panel.toml")
}

/// The size of those allowed nearest (w, h), a row off counting as two columns.
fn nearest(sizes: &[(u8, u8)], w: i32, h: i32) -> (u8, u8) {
    let d = |&(sw, sh): &(u8, u8)| (sw as i32 - w).pow(2) + 4 * (sh as i32 - h).pow(2);
    sizes.iter().copied().min_by_key(d).unwrap_or((BASE, 1))
}

/// The size a corner dragged by (dw, dh) cells (fractions of them) from (w, h) takes: of the allowed sizes
/// the drag heads for, the one it has come nearest to, once it is 40% of the way there; else the size it was.
/// So a toggle of 4 grows to 8 at 1.6 cells, not past the 6 the nearest size alone would wait for.
fn toward(sizes: &[(u8, u8)], (w, h): (u8, u8), (dw, dh): (f64, f64)) -> (u8, u8) {
    let mut best = ((w, h), f64::MAX);
    for &(sw, sh) in sizes {
        let (vw, vh) = (sw as f64 - w as f64, sh as f64 - h as f64);
        let len = vw * vw + vh * vh;
        if len == 0.0 {
            continue;
        }
        // how far along the way to this size the drag is, 1 there
        let along = (dw * vw + dh * vh) / len;
        if along >= 0.4 && (along - 1.0).abs() < best.1 {
            best = ((sw, sh), (along - 1.0).abs());
        }
    }
    best.0
}

/// A panel's layout of eight as panel.toml has it (widgets unknown dropped, sizes not allowed made the nearest
/// allowed), else its spec's; the widgets in the bar alone, where they were; and when its badges show where it says.
fn load(reg: &[Meta], spec: &Spec) -> (Vec<Item>, Vec<Item>, HashMap<String, Show>) {
    let mut saved = read_saved().unwrap_or_default();
    let placed = saved.remove(&spec.id).or_else(|| if spec.id == "control" { saved.remove("widget") } else { None });
    let mut shows = HashMap::new();
    let mut hidden = Vec::new();
    let mut items: Vec<Item> = match placed {
        Some(ps) => ps
            .into_iter()
            .filter_map(|p| {
                let m = reg.iter().find(|m| m.id == p.id)?;
                let (w, h) = nearest(m.sizes, p.w as i32, p.h as i32);
                if let Some(b) = p.bar {
                    shows.insert(p.id.clone(), b);
                }
                let it = Item { key: p.id, x: p.x, y: p.y, w, h };
                if p.hidden {
                    hidden.push(it);
                    return None;
                }
                Some(it)
            })
            .collect(),
        // the spec's, of the widgets there are (a plugin or a KDL file gone, its place empty no more)
        None => {
            let mut l: Vec<Item> = spec.layout.iter().filter(|i| reg.iter().any(|m| m.id == i.key)).cloned().collect();
            grid::rescale(&mut l, spec.cols, BASE);
            l
        }
    };
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.key.clone()));
    hidden.retain(|i| seen.insert(i.key.clone()));
    grid::settle(&mut items, BASE);
    (items, hidden, shows)
}

/// A panel's layout into panel.toml, the other panels' kept.
fn save(id: &str, items: &[Item], hidden: &[Item], shows: &HashMap<String, Show>, aligns: &HashMap<String, String>, lively: &std::collections::HashSet<String>) {
    let mut s = read_saved().unwrap_or_default();
    s.remove("widget");
    let all = items.iter().map(|i| (i, false)).chain(hidden.iter().map(|i| (i, true)));
    let placed = all.map(|(i, hidden)| Placed {
        id: i.key.clone(),
        x: i.x,
        y: i.y,
        w: i.w,
        h: i.h,
        bar: shows.get(&i.key).copied(),
        hidden,
        align: aligns.get(&i.key).filter(|a| *a != "fill").cloned(),
        while_active: lively.contains(&i.key),
    });
    s.insert(id.to_string(), placed.collect());
    let text = match toml::to_string(&s) {
        Ok(t) => format!("# ostrov's panels, as their Edit leaves them\n\n{t}"),
        Err(e) => return eprintln!("ostrov: panel layout: {e}"),
    };
    if let Err(e) = std::fs::write(layout_path(), text) {
        eprintln!("ostrov: {}: {e}", layout_path().display());
    }
}

/// A widget on the grid: what was made, its tile (the widget under its editing's minus and corner), its menu
/// in its revealer.
struct Tile {
    widget: Widget,
    wrap: gtk4::Overlay,
    menu: Option<gtk4::Revealer>,
}

/// The tile dragged in the editing: it as the drag found it, by its corner (sized) or not (moved), the layout
/// the drag started from, the cells it was last put at, a cell's pitch as the tile is laid out (across, down).
type Dragged = (Item, bool, Vec<Item>, (u8, u8, u8, u8), (f64, f64));

pub struct Panel {
    pub popup: Rc<Popup>,
    spec: Spec,
    reg: Rc<Vec<Meta>>,
    /// its face in the bar: its widgets' badges, else its icon (the other bars show it as drawn)
    pub face: gtk4::Box,
    face_icon: gtk4::Image,
    shows: RefCell<HashMap<String, Show>>,
    /// how each widget lines up across its tile ("fill" unless said)
    aligns: RefCell<HashMap<String, String>>,
    /// the widgets on it only while active, and those of them shown as last laid out
    lively: RefCell<std::collections::HashSet<String>>,
    lively_shown: RefCell<Vec<String>>,
    /// the grid's page, the Settings', the Appearance's
    pages: gtk4::Stack,
    settings: Option<Rc<crate::settings::form::Page>>,
    /// a row's height and the gap between cells, in pixels, as the density says
    dims: Cell<(i32, i32)>,
    grid: gtk4::Grid,
    gallery: gtk4::Box,
    /// the widgets in the bar alone, under the grid in the editing
    shelf: gtk4::Box,
    /// in the editing, what can be done to the tile picked (a click on it): its size, its badge, its settings
    inspector: gtk4::Box,
    picked: RefCell<String>,
    edit_button: gtk4::Button,
    items: RefCell<Vec<Item>>,
    /// its grid's width in cells, and its layout as one of eight, what any width is laid out from
    cols: Cell<u8>,
    base: RefCell<Vec<Item>>,
    /// the widgets in the bar alone, kept where they were on the grid (their badges' order)
    hidden: RefCell<Vec<Item>>,
    tiles: RefCell<HashMap<String, Tile>>,
    open: RefCell<String>,
    editing: Cell<bool>,
    state: Rc<RefCell<Value>>,
}

impl Panel {
    pub fn toggle(&self) {
        self.popup.toggle()
    }

    /// Open with one widget's menu unfolded (ostrov menu wifi: a key to it, or a look without a click); the
    /// old panel's names for theirs still work. "edit" opens it in its editing.
    pub fn open_menu(self: &Rc<Self>, name: &str) {
        // that menu open already: closed (a key that opens it closes it again)
        if self.popup.is_open() && name != "edit" && *self.open.borrow() == alias(name) {
            return self.popup.close();
        }
        self.popup.open();
        if name == "edit" {
            return self.set_editing(true);
        }
        self.set_open(alias(name));
    }

    /// A widget on the grid made w×h, a size it allows, saved.
    pub fn resize(self: &Rc<Self>, key: &str, w: u8, h: u8) -> Result<(), String> {
        let m = self.reg.iter().find(|m| m.id == key).ok_or(format!("no widget {key}"))?;
        let sizes = self.sizes(m);
        if !sizes.contains(&(w, h)) {
            let all: Vec<String> = sizes.iter().map(|(w, h)| format!("{w}×{h}")).collect();
            return Err(format!("{key} is {}", all.join(", ")));
        }
        let it = self.items.borrow().iter().find(|i| i.key == key).cloned().ok_or(format!("{key} is not on the grid"))?;
        let mut items = self.items.borrow().clone();
        grid::place(&mut items, key, it.x, it.y, w, h, self.cols.get());
        *self.items.borrow_mut() = items;
        self.layout();
        self.faces();
        self.keep();
        Ok(())
    }

    /// Open in the editing, the widget key picked ("" none).
    pub fn edit(self: &Rc<Self>, key: &str) {
        self.open_menu("edit");
        self.pick(key);
    }

    /// Whether a widget key is on it.
    pub fn has(&self, key: &str) -> bool {
        self.items.borrow().iter().chain(self.hidden.borrow().iter()).any(|i| i.key == key)
    }

    /// Open at a page, or closed if it is open at it already (a key that opens it closes it again): "settings"
    /// (at an entry's form, if given), "appearance".
    pub fn toggle_page(&self, page: &str, entry: Option<&str>) {
        if self.popup.is_open() && self.pages.visible_child_name().as_deref() == Some(page) && entry.is_none() {
            return self.popup.close();
        }
        self.open_page(page, entry);
    }

    /// Open at a page: "settings" (at an entry's form, if given), "appearance", "grid".
    pub fn open_page(&self, page: &str, entry: Option<&str>) {
        if !self.popup.is_open() {
            self.popup.open();
        }
        if let Some(s) = self.settings.as_ref().filter(|_| page == "settings") {
            s.show(entry);
        }
        if !(page == "settings" && matches!(entry, Some("appearance" | "bar"))) {
            self.pages.set_visible_child_full(page, gtk4::StackTransitionType::None);
        }
    }

    /// The rows as the density in the config says, laid out again if that changed.
    fn fit_density(&self) {
        let (row, gap) = crate::look::density(&crate::config::load().appearance);
        if self.dims.replace((row, gap)) != (row, gap) {
            self.grid.set_column_spacing(gap as u32);
            self.grid.set_row_spacing(gap as u32);
            self.layout();
        }
    }

    /// The widget key made, in its tile.
    fn tile(self: &Rc<Self>, key: &str) -> Option<Tile> {
        let m = self.reg.iter().find(|m| m.id == key)?;
        let me = Rc::downgrade(self);
        let k = key.to_string();
        let flip: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(p) = me.upgrade() {
                let now = if *p.open.borrow() == k { String::new() } else { k.clone() };
                p.set_open(&now);
            }
        });
        let me = Rc::downgrade(self);
        let again: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(p) = me.upgrade() {
                p.draw();
            }
        });
        let me = Rc::downgrade(self);
        let close: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(p) = me.upgrade() {
                p.popup.close();
            }
        });
        let widget = (m.make)(&Ctx { close, flip, again, state: self.state.clone() });
        widget.root.set_hexpand(true);
        widget.root.set_vexpand(true);
        widget.root.set_halign(align(self.aligns.borrow().get(key).map_or("fill", String::as_str)));

        let wrap = gtk4::Overlay::new();
        wrap.add_css_class("tile");
        wrap.set_widget_name(key);
        // the tile as tall as its cells and no taller: what it holds clipped to them (scrolled, a scroll of its
        // own aside), never a row pushed taller than its neighbours
        let clip = gtk4::ScrolledWindow::new();
        // across too: what it holds asks no width of the grid, which its cells alone set (a tile's text wider than
        // its cells cut, never the panel made wider, a width of 6 wider than one of 7)
        clip.set_policy(gtk4::PolicyType::External, gtk4::PolicyType::External);
        clip.set_propagate_natural_width(true);
        clip.set_child(Some(&widget.root));
        // a card's ground on the tile itself, its cells' size whatever it holds: what is cut off is inside it
        for ground in ["card", "clock"] {
            if widget.root.has_css_class(ground) {
                widget.root.remove_css_class(ground);
                widget.root.add_css_class(&format!("{ground}-body"));
                clip.add_css_class(ground);
            }
        }
        wrap.set_child(Some(&clip));
        // its widget's stretch kept within the tile: let up to the grid it would hand the grid's spare height to
        // its rows, making them taller than a cell
        wrap.set_vexpand(false);
        wrap.set_hexpand(false);
        let remove = gtk4::Button::from_icon_name("list-remove-symbolic");
        remove.add_css_class("tile-remove");
        remove.set_halign(Align::Start);
        remove.set_valign(Align::Start);
        let me = Rc::downgrade(self);
        let k = key.to_string();
        remove.connect_clicked(move |_| {
            if let Some(p) = me.upgrade() {
                p.remove(&k);
            }
        });
        let grip = gtk4::Image::from_icon_name("view-fullscreen-symbolic");
        grip.add_css_class("tile-grip");
        grip.set_halign(Align::End);
        grip.set_valign(Align::End);
        let marks = [remove.upcast::<gtk4::Widget>(), grip.upcast()];
        for w in &marks {
            w.set_visible(self.editing.get());
            wrap.add_overlay(w);
        }
        widget.root.set_can_target(!self.editing.get());

        // a gear in its menu's head to its settings, a widget's that has some
        let plugin = key.strip_prefix("plugin.").and_then(|r| r.split('.').next()).map(|p| format!("plugin.{p}"));
        let entry = [Some(format!("widget.{key}")), plugin].into_iter().flatten().find(|e| crate::settings::entry(e).is_some());
        if let (Some(card), Some(entry)) = (widget.menu.as_ref(), entry)
            && let Some(head) = card.first_child().and_downcast::<gtk4::Box>()
        {
            let gear = gtk4::Button::from_icon_name("emblem-system-symbolic");
            gear.add_css_class("flat-round");
            gear.set_tooltip_text(Some(t("Settings")));
            gear.set_hexpand(true);
            gear.set_halign(Align::End);
            let me = Rc::downgrade(self);
            gear.connect_clicked(move |_| {
                if let Some(p) = me.upgrade() {
                    p.open_page("settings", Some(&entry));
                }
            });
            head.append(&gear);
        }
        let menu = widget.menu.as_ref().map(|card| {
            let r = gtk4::Revealer::new();
            r.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
            r.set_transition_duration(100);
            r.set_child(Some(card));
            r.set_visible(false);
            // shown before it slides open, hidden once it has slid shut: a folded menu takes no room
            r.connect_child_revealed_notify(|r| {
                if !r.is_child_revealed() && !r.reveals_child() {
                    r.set_visible(false);
                    if let Some(band) = r.parent() {
                        fit_band(&band);
                    }
                }
            });
            r
        });
        Some(Tile { widget, wrap, menu })
    }

    /// The grid laid out anew from the items: each tile at its cells, and under each band of rows whose widgets
    /// have menus a row of their own for those menus, shown only while one of them is unfolded.
    /// The tiles on the grid now: all of them while it is edited; else those not on it only while active, and
    /// those whose widgets are, the others closing up their places.
    fn shown(&self) -> Vec<Item> {
        let lively = self.lively.borrow();
        if self.editing.get() || lively.is_empty() {
            return self.items.borrow().clone();
        }
        let tiles = self.tiles.borrow();
        let active = |k: &str| tiles.get(k).and_then(|t| t.widget.face.as_ref()).is_none_or(|f| f.active.get());
        let mut items: Vec<Item> =
            self.items.borrow().iter().filter(|i| !lively.contains(&i.key) || active(&i.key)).cloned().collect();
        grid::compact(&mut items, self.cols.get());
        items
    }

    fn layout(&self) {
        while let Some(c) = self.grid.first_child() {
            self.grid.remove(&c);
        }
        let items = self.shown();
        *self.lively_shown.borrow_mut() =
            items.iter().filter(|i| self.lively.borrow().contains(&i.key)).map(|i| i.key.clone()).collect();
        let tiles = self.tiles.borrow();
        let (row, gap) = self.dims.get();
        let mut bands: Vec<(u8, gtk4::Box)> = Vec::new();
        for it in items.iter() {
            let Some(r) = tiles.get(&it.key).and_then(|t| t.menu.as_ref()) else { continue };
            if let Some(old) = r.parent().and_downcast::<gtk4::Box>() {
                old.remove(r);
            }
            let b = grid::below(&items, it.y, it.h);
            let i = match bands.iter().position(|(at, _)| *at == b) {
                Some(i) => i,
                None => {
                    bands.push((b, gtk4::Box::new(Orientation::Vertical, 0)));
                    bands.len() - 1
                }
            };
            bands[i].1.append(r);
        }
        // a row's place in the grid: past the bands above it
        let above = |row: u8, or_at: bool| bands.iter().filter(|(b, _)| *b < row || or_at && *b == row).count() as i32;
        // a row of the grid a cell high whatever its tiles: an empty strut in each, under the tiles, so GtkGrid
        // never has a tall tile's height to share out over rows (which it does in its children's order, a tile
        // left taller or shorter than its cells, a neighbour moved as another is dragged)
        let height = items.iter().map(|i| i.y + i.h).max().unwrap_or(0);
        for r in 0..height {
            let strut = gtk4::Box::new(Orientation::Vertical, 0);
            strut.set_size_request(-1, row);
            strut.set_can_target(false);
            self.grid.attach(&strut, 0, r as i32 + above(r, true), 1, 1);
        }
        let mut order: Vec<&Item> = items.iter().collect();
        order.sort_by_key(|i| (i.h, i.y, i.x));
        for it in order {
            if let Some(t) = tiles.get(&it.key) {
                t.wrap.set_size_request(-1, it.h as i32 * row + (it.h as i32 - 1) * gap);
                self.grid.attach(&t.wrap, it.x as i32, it.y as i32 + above(it.y, true), it.w as i32, it.h as i32);
                // told its cells, each as wide on any panel: what it shows chosen by its width (a toggle's words
                // from 3)
                (t.widget.size)(it.w, it.h);
            }
        }
        for (b, bx) in &bands {
            fit_band(bx.upcast_ref());
            self.grid.attach(bx, 0, *b as i32 + above(*b, false), self.cols.get() as i32, 1);
        }    }

    /// One widget's menu unfolded, the others folded ("" all).
    fn set_open(&self, key: &str) {
        *self.open.borrow_mut() = key.to_string();
        for (k, t) in self.tiles.borrow().iter() {
            if k == key {
                t.wrap.add_css_class("open");
            } else {
                t.wrap.remove_css_class("open");
            }
            if let Some(r) = &t.menu {
                if k == key {
                    r.set_visible(true);
                    if let Some(band) = r.parent() {
                        band.set_visible(true);
                    }
                }
                r.set_reveal_child(k == key);
            }
        }
    }

    fn draw(&self) {
        let st = self.state.borrow().clone();
        for t in self.tiles.borrow().values() {
            (t.widget.draw)(&st);
            // its hover its badge's: the battery's time left, a toggle's state, a slider's level
            if let Some(f) = &t.widget.face {
                let tip = f.root.tooltip_text();
                if t.widget.root.tooltip_text() != tip {
                    t.widget.root.set_tooltip_text(tip.as_deref());
                }
            }
        }
        self.fit_faces();
    }

    /// When a widget's badge shows here: as the panel says, else as the widget does.
    fn show(&self, key: &str, default: Show) -> Show {
        self.shows.borrow().get(key).copied().unwrap_or(default)
    }

    /// The face laid anew: the badges in their widgets' order on the grid, left to right, top to bottom.
    fn faces(&self) {
        while let Some(c) = self.face.first_child() {
            self.face.remove(&c);
        }
        let mut items = self.items.borrow().clone();
        items.extend(self.hidden.borrow().iter().cloned());
        items.sort_by_key(|i| (i.y, i.x));
        let tiles = self.tiles.borrow();
        for f in items.iter().filter_map(|i| tiles.get(&i.key)?.widget.face.as_ref()) {
            self.face.append(&f.root);
        }
        self.face.append(&self.face_icon);
        drop(tiles);
        self.fit_faces();
    }

    /// Each badge shown or not, as when it shows says and its widget's activity; the icon while none is.
    fn fit_faces(&self) {
        // a tile on the panel only while active come or gone: the grid laid out again
        let lively: Vec<String> = self.shown().into_iter().filter(|i| self.lively.borrow().contains(&i.key)).map(|i| i.key).collect();
        if lively != *self.lively_shown.borrow() {
            self.layout();
        }
        let mut any = false;
        for (k, t) in self.tiles.borrow().iter() {
            let Some(f) = &t.widget.face else { continue };
            let Some(m) = self.reg.iter().find(|m| m.id == k) else { continue };
            let on = match self.show(k, m.bar) {
                Show::Always => true,
                Show::Active => f.active.get(),
                Show::Never => false,
            };
            f.root.set_visible(on);
            any |= on;
        }
        self.face_icon.set_visible(!any);
    }

    /// In the editing or out of it, the layout saved on the way out.
    /// A widget's sizes on this panel's grid.
    fn sizes(&self, m: &Meta) -> Vec<(u8, u8)> {
        grid::fit(m.sizes, self.cols.get())
    }

    /// The grid made n cells wide: its layout rescaled, the panel as wide, both saved.
    pub fn set_cols(self: &Rc<Self>, n: u8) {
        let n = n.clamp(MIN_COLS, MAX_COLS);
        let from = self.cols.replace(n);
        if from == n {
            return;
        }
        // laid out anew from the layout of eight, the last one edited: back and forth, nothing drifts
        let mut base = self.base.borrow().clone();
        let mut was = base.clone();
        grid::rescale(&mut was, BASE, from);
        if was != *self.items.borrow() {
            base = self.items.borrow().clone();
            grid::rescale(&mut base, from, BASE);
            *self.base.borrow_mut() = base.clone();
        }
        grid::rescale(&mut base, BASE, n);
        *self.items.borrow_mut() = base;
        self.popup.set_width(self.spec.width_for(n));
        if let Err(e) = crate::settings::write(&format!("panels.{}", self.spec.id), "cols", Some(&serde_json::json!(n)), true) {
            eprintln!("ostrov: panel: {e}");
        }
        save(&self.spec.id, &self.base.borrow(), &self.hidden.borrow(), &self.shows.borrow(), &self.aligns.borrow(), &self.lively.borrow());
        self.layout();
        self.faces();
        self.fill_gallery();
    }

    /// The layout as edited kept: as the layout of eight it is laid out from, and in panel.toml.
    fn keep(&self) {
        let mut base = self.items.borrow().clone();
        grid::rescale(&mut base, self.cols.get(), BASE);
        *self.base.borrow_mut() = base;
        save(&self.spec.id, &self.base.borrow(), &self.hidden.borrow(), &self.shows.borrow(), &self.aligns.borrow(), &self.lively.borrow());
        // its width kept with it: a saved layout without one is taken for one from before widths, eight
        let cols = serde_json::json!(self.cols.get());
        if let Err(e) = crate::settings::write(&format!("panels.{}", self.spec.id), "cols", Some(&cols), true) {
            eprintln!("ostrov: panel: {e}");
        }
    }

    fn set_editing(self: &Rc<Self>, on: bool) {
        if !on && self.editing.get() {
            self.keep();
        }
        self.editing.set(on);
        self.set_open("");
        self.edit_button.set_label(t(if on { "Done" } else { "Edit" }));
        for t in self.tiles.borrow().values() {
            t.widget.root.set_can_target(!on);
            let mut c = t.wrap.first_child();
            while let Some(w) = c {
                if w.has_css_class("tile-remove") || w.has_css_class("tile-grip") {
                    w.set_visible(on);
                }
                c = w.next_sibling();
            }
        }
        if on {
            self.grid.add_css_class("editing");
        } else {
            self.grid.remove_css_class("editing");
        }
        self.pick("");
        self.fill_gallery();
        self.gallery.set_visible(on);
        // every tile while edited, those only while active among them; as they are after
        self.layout();
    }

    fn remove(self: &Rc<Self>, key: &str) {
        if *self.picked.borrow() == key {
            self.pick("");
        }
        self.items.borrow_mut().retain(|i| i.key != key);
        self.hidden.borrow_mut().retain(|i| i.key != key);
        grid::compact(&mut self.items.borrow_mut(), self.cols.get());
        self.tiles.borrow_mut().remove(key);
        self.layout();
        self.faces();
        self.fill_gallery();
    }

    /// The tile picked in the editing ("" none): ringed, its inspector under the grid.
    fn pick(self: &Rc<Self>, key: &str) {
        *self.picked.borrow_mut() = key.to_string();
        for (k, t) in self.tiles.borrow().iter() {
            if k == key {
                t.wrap.add_css_class("picked");
            } else {
                t.wrap.remove_css_class("picked");
            }
        }
        self.fill_shelf();
        self.inspect();
    }

    /// The picked tile's inspector: its sizes, when its badge is in the bar, the bar alone, its settings, off.
    fn inspect(self: &Rc<Self>) {
        clear(&self.inspector);
        let key = self.picked.borrow().clone();
        let on = self.items.borrow().iter().find(|i| i.key == key).cloned();
        let off = on.is_none();
        let it = on.or_else(|| self.hidden.borrow().iter().find(|i| i.key == key).cloned());
        let (Some(it), Some(m)) = (it, self.reg.iter().find(|m| m.id == key)) else {
            return self.inspector.set_visible(false);
        };
        self.inspector.set_visible(true);
        let head = gtk4::Box::new(Orientation::Horizontal, 10);
        let badge = gtk4::Image::from_icon_name(m.icon);
        badge.add_css_class("badge");
        head.append(&badge);
        head.append(&label(m.name, "title"));
        self.inspector.append(&head);

        let line = |title: &'static str, w: &gtk4::Widget| {
            // its words over its chips, the chips wrapping: as narrow as the panel goes
            let bx = gtk4::Box::new(Orientation::Vertical, 4);
            bx.append(&label(t(title), "dim"));
            bx.append(w);
            self.inspector.append(&bx);
        };
        // its sizes, the one it has pressed
        let sizes = self.sizes(m);
        if sizes.len() > 1 && !off {
            let names: Vec<String> = sizes.iter().map(|(w, h)| format!("{w}×{h}")).collect();
            let strs: Vec<&str> = names.iter().map(String::as_str).collect();
            let at = sizes.iter().position(|&s| s == (it.w, it.h));
            let (me, k) = (Rc::downgrade(self), key.clone());
            let chips = crate::ui::chips(&strs, at, move |i| {
                let Some(p) = me.upgrade() else { return };
                let Some(it) = p.items.borrow().iter().find(|x| x.key == k).cloned() else { return };
                let (w, h) = sizes[i];
                let mut items = p.items.borrow().clone();
                grid::place(&mut items, &k, it.x, it.y, w, h, p.cols.get());
                *p.items.borrow_mut() = items;
                p.layout();
                p.faces();
                p.inspect();
            });
            line("Size", chips.upcast_ref());
        }
        // when its badge is in the bar, and the bar alone
        if self.tiles.borrow().get(&key).is_some_and(|t| t.widget.face.is_some()) {
            let now = self.show(&key, m.bar);
            let names: Vec<&str> = Show::ALL.iter().map(|s| t(s.1)).collect();
            let at = Show::ALL.iter().position(|s| s.0 == now);
            let (me, k) = (Rc::downgrade(self), key.clone());
            let chips = crate::ui::chips(&names, at, move |i| {
                if let Some(p) = me.upgrade() {
                    p.shows.borrow_mut().insert(k.clone(), Show::ALL[i].0);
                    p.faces();
                    p.inspect();
                }
            });
            line("In the bar", chips.upcast_ref());
        }
        // on the panel always, or only while its widget is active
        let has_face = self.tiles.borrow().get(&key).is_some_and(|t| t.widget.face.is_some());
        if !off && has_face {
            let now = usize::from(self.lively.borrow().contains(&key));
            let (me, k) = (Rc::downgrade(self), key.clone());
            let chips = crate::ui::chips(&[t("Always"), t("While active")], Some(now), move |i| {
                let Some(p) = me.upgrade() else { return };
                if i == 1 {
                    p.lively.borrow_mut().insert(k.clone());
                } else {
                    p.lively.borrow_mut().remove(&k);
                }
                p.inspect();
            });
            line("On the panel", chips.upcast_ref());
        }
        // how it lines up across its tile
        if !off {
            let now = self.aligns.borrow().get(&key).cloned().unwrap_or_else(|| "fill".into());
            let names: Vec<&str> = ALIGNS.iter().map(|a| t(a.2)).collect();
            let at = ALIGNS.iter().position(|a| a.0 == now);
            let (me, k) = (Rc::downgrade(self), key.clone());
            let chips = crate::ui::chips(&names, at, move |i| {
                let Some(p) = me.upgrade() else { return };
                p.aligns.borrow_mut().insert(k.clone(), ALIGNS[i].0.to_string());
                if let Some(t) = p.tiles.borrow().get(&k) {
                    t.widget.root.set_halign(ALIGNS[i].1);
                }
                p.inspect();
            });
            line("Align", chips.upcast_ref());
        }
        let acts = crate::ui::chip_flow();
        let act = |text: &'static str, f: Box<dyn Fn(&Rc<Panel>)>| {
            let b = crate::ui::chip(t(text));
            let me = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(p) = me.upgrade() {
                    f(&p);
                }
            });
            crate::ui::flow_in(&acts, &b);
        };
        let k = key.clone();
        if off {
            act("Back on the Panel", Box::new(move |p| p.unhide(&k)));
        } else if self.tiles.borrow().get(&key).is_some_and(|t| t.widget.face.is_some()) {
            act("In the Bar Only", Box::new(move |p| p.hide(&k)));
        }
        let k = key.clone();
        act("Remove", Box::new(move |p| p.remove(&k)));
        self.inspector.append(&acts);
        // its settings right here, written to the config as they change: its own (a module's, a KDL widget's
        // [widget.ID]) or its plugin's ([plugin.ID])
        let plugin = key.strip_prefix("plugin.").and_then(|r| r.split('.').next()).map(|p| format!("plugin.{p}"));
        if let Some(e) = crate::settings::entry(&format!("widget.{key}")).or_else(|| plugin.and_then(|p| crate::settings::entry(&p))) {
            self.inspector.append(&gtk4::Separator::new(Orientation::Horizontal));
            self.inspector.append(&label(t("Settings"), "dim"));
            self.inspector.append(&crate::settings::form::form(&e.schema));
        }
    }

    /// A widget's tile off the panel, its badge left in the bar (shown always, if it was never).
    fn hide(self: &Rc<Self>, key: &str) {
        let Some(it) = self.items.borrow().iter().find(|i| i.key == key).cloned() else { return };
        self.pick("");
        self.items.borrow_mut().retain(|i| i.key != key);
        grid::compact(&mut self.items.borrow_mut(), self.cols.get());
        self.hidden.borrow_mut().push(it);
        let bar = self.reg.iter().find(|m| m.id == key).map_or(Show::Never, |m| m.bar);
        if self.show(key, bar) == Show::Never {
            self.shows.borrow_mut().insert(key.into(), Show::Always);
        }
        self.layout();
        self.faces();
        self.fill_gallery();
    }

    /// A widget in the bar alone back on the panel, in the first place its size fits.
    fn unhide(self: &Rc<Self>, key: &str) {
        let Some(mut it) = self.hidden.borrow().iter().find(|i| i.key == key).cloned() else { return };
        self.pick("");
        self.hidden.borrow_mut().retain(|i| i.key != key);
        (it.x, it.y) = grid::free(&self.items.borrow(), it.w, it.h, self.cols.get());
        self.items.borrow_mut().push(it);
        self.layout();
        self.faces();
        self.fill_gallery();
    }

    /// A widget put on the grid at its default size, in the first place it fits.
    fn add(self: &Rc<Self>, key: &str) {
        let Some(m) = self.reg.iter().find(|m| m.id == key) else { return };
        let Some(t) = self.tile(key) else { return };
        let (w, h) = self.sizes(m).first().copied().unwrap_or((self.cols.get(), 1));
        let (x, y) = grid::free(&self.items.borrow(), w, h, self.cols.get());
        self.items.borrow_mut().push(Item { key: key.into(), x, y, w, h });
        self.tiles.borrow_mut().insert(key.into(), t);
        self.layout();
        self.faces();
        self.fill_gallery();
        self.draw();
    }

    /// The shelf under the grid in the editing, past a rule: the widgets on in the bar alone, a chip each, a
    /// click picking one (its inspector puts it back on the panel).
    fn fill_shelf(self: &Rc<Self>) {
        clear(&self.shelf);
        let hidden = self.hidden.borrow().clone();
        self.shelf.set_visible(self.editing.get() && !hidden.is_empty());
        self.shelf.append(&gtk4::Separator::new(Orientation::Horizontal));
        self.shelf.append(&label(t("In the bar only"), "dim"));
        let chips = crate::ui::chip_flow();
        for it in hidden {
            let Some(m) = self.reg.iter().find(|m| m.id == it.key) else { continue };
            let chip = gtk4::ToggleButton::new();
            chip.add_css_class("chip");
            let bx = gtk4::Box::new(Orientation::Horizontal, 6);
            bx.append(&gtk4::Image::from_icon_name(m.icon));
            bx.append(&gtk4::Label::new(Some(m.name)));
            chip.set_child(Some(&bx));
            chip.set_active(*self.picked.borrow() == it.key);
            let me = Rc::downgrade(self);
            chip.connect_clicked(move |_| {
                if let Some(p) = me.upgrade() {
                    let now = if *p.picked.borrow() == it.key { String::new() } else { it.key.clone() };
                    p.pick(&now);
                }
            });
            crate::ui::flow_in(&chips, &chip);
        }
        self.shelf.append(&chips);
    }

    /// The gallery: every widget not on the grid, its sizes, a click putting it on.
    fn fill_gallery(self: &Rc<Self>) {
        clear(&self.gallery);
        let hidden: Vec<String> = self.hidden.borrow().iter().map(|i| i.key.clone()).collect();
        self.fill_shelf();
        // the panel's own: its width in cells, its name and icon folded
        self.gallery.append(&label(t("Panel"), "title"));
        let width = gtk4::Box::new(Orientation::Horizontal, 6);
        width.add_css_class("gallery-line");
        let name = label(t("Width"), "");
        name.set_hexpand(true);
        width.append(&name);
        for (icon, by) in [("list-remove-symbolic", -1i8), ("list-add-symbolic", 1)] {
            let b = gtk4::Button::from_icon_name(icon);
            b.add_css_class("flat-round");
            let me = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(p) = me.upgrade() {
                    p.set_cols(p.cols.get().saturating_add_signed(by));
                }
            });
            if by < 0 {
                b.set_sensitive(self.cols.get() > MIN_COLS);
                width.append(&b);
                width.append(&label(&self.cols.get().to_string(), "bold"));
            } else {
                b.set_sensitive(self.cols.get() < MAX_COLS);
                width.append(&b);
            }
        }
        self.gallery.append(&width);
        let schema = crate::settings::panel_schema(&self.spec.id, &self.spec.name, &self.spec.icon, None);
        self.gallery.append(&crate::ui::fold(t("Name and Icon"), &crate::settings::form::form(&schema)));
        self.gallery.append(&gtk4::Separator::new(Orientation::Horizontal));
        self.gallery.append(&label(t("Add Widgets"), "title"));
        let mut on: Vec<String> = self.items.borrow().iter().map(|i| i.key.clone()).collect();
        on.extend(hidden);
        let mut any = false;
        for m in self.reg.iter().filter(|m| !on.iter().any(|k| k == m.id)) {
            any = true;
            let sizes: Vec<String> = self.sizes(m).iter().map(|(w, h)| format!("{w}×{h}")).collect();
            let me = Rc::downgrade(self);
            let id = m.id;
            self.gallery.append(&crate::ui::row(m.icon, m.name, &sizes.join(" "), false, move || {
                if let Some(p) = me.upgrade() {
                    p.add(id);
                }
            }));
        }
        if !any {
            self.gallery.append(&label(t("Every widget is on the panel"), "dim"));
        }
    }

    /// Dragging a tile in the editing: by its body to move it, by its corner to size it, a frame following the pointer
    /// as it goes and the grid taking the cells it comes to under it. The grid's own drag, in the grid's coordinates,
    /// since the tile moves under the pointer. The tile dragged ringed (style.rs), its minus and corner over the ring. A press on a tile's minus is its own; a click, no drag, picks the tile.
    fn drags(self: &Rc<Self>) {
        let drag = gtk4::GestureDrag::new();
        let at: Rc<RefCell<Option<Dragged>>> = Rc::default();
        let (me, a) = (Rc::downgrade(self), at.clone());
        drag.connect_drag_begin(move |g, x, y| {
            let Some(p) = me.upgrade().filter(|p| p.editing.get()) else {
                g.set_state(gtk4::EventSequenceState::Denied);
                return;
            };
            let mut hit = p.grid.pick(x, y, gtk4::PickFlags::DEFAULT);
            let (mut corner, mut key) = (false, None);
            while let Some(w) = hit {
                if w.has_css_class("tile-remove") {
                    g.set_state(gtk4::EventSequenceState::Denied);
                    return;
                }
                corner |= w.has_css_class("tile-grip");
                if w.has_css_class("tile") {
                    key = Some(w.widget_name().to_string());
                    break;
                }
                hit = w.parent();
            }
            let items = p.items.borrow().clone();
            let Some(it) = key.and_then(|k| items.iter().find(|i| i.key == k).cloned()) else {
                g.set_state(gtk4::EventSequenceState::Denied);
                return;
            };
            g.set_state(gtk4::EventSequenceState::Claimed);
            // a cell's pitch from the tile as laid out, whatever the grid's margins: its size over its cells
            let (row, gap) = p.dims.get();
            let mut pitch = ((p.grid.width() + gap) as f64 / p.cols.get() as f64, (row + gap) as f64);
            if let Some(t) = p.tiles.borrow().get(&it.key) {
                t.wrap.add_css_class("dragged");
                if let Some(b) = t.wrap.compute_bounds(&p.grid) {
                    pitch = ((b.width() as f64 + gap as f64) / it.w as f64, (b.height() as f64 + gap as f64) / it.h as f64);
                }
            }
            let last = (it.x, it.y, it.w, it.h);
            *a.borrow_mut() = Some((it, corner, items, last, pitch));
        });
        let (me, a) = (Rc::downgrade(self), at.clone());
        drag.connect_drag_update(move |_, dx, dy| {
            let Some(p) = me.upgrade() else { return };
            let mut a = a.borrow_mut();
            let Some((it, corner, start, last, pitch)) = a.as_mut() else { return };
            let Some(m) = p.reg.iter().find(|m| m.id == it.key) else { return };
            let (fx, fy) = (dx / pitch.0, dy / pitch.1);
            let (cx, cy) = (fx.round() as i32, fy.round() as i32);
            let next = if *corner {
                let (nw, nh) = toward(&p.sizes(m), (it.w, it.h), (fx, fy));
                (it.x, it.y, nw, nh)
            } else {
                let x = (it.x as i32 + cx).clamp(0, (p.cols.get() - it.w) as i32) as u8;
                (x, (it.y as i32 + cy).max(0) as u8, it.w, it.h)
            };
            if next == *last {
                return;
            }
            *last = next;
            let mut items = start.clone();
            grid::place(&mut items, &it.key, next.0, next.1, next.2, next.3, p.cols.get());
            *p.items.borrow_mut() = items;
            p.layout();
            p.faces();
        });
        let (me, a) = (Rc::downgrade(self), at);
        drag.connect_drag_end(move |_, dx, dy| {
            let (Some(p), Some((it, ..))) = (me.upgrade(), a.borrow_mut().take()) else { return };
            // a click, not a drag: the tile picked (again: let go)
            if dx.abs() < 4.0 && dy.abs() < 4.0 {
                let now = if *p.picked.borrow() == it.key { String::new() } else { it.key.clone() };
                p.pick(&now);
            }
            if let Some(t) = p.tiles.borrow().get(&it.key) {
                t.wrap.remove_css_class("dragged");
            }
        });
        self.grid.add_controller(drag);
    }
}

/// A band of menus shown only while one of its menus is: a row all hidden takes no gap in the grid.
fn fit_band(bx: &gtk4::Widget) {
    let mut c = bx.first_child();
    let mut any = false;
    while let Some(w) = c {
        any |= w.is_visible();
        c = w.next_sibling();
    }
    bx.set_visible(any);
}

/// A widget by the name its menu had in the panel before the grid (ostrov menu outs), or by its own.
pub fn alias(name: &str) -> &str {
    ALIASES.iter().find(|(old, _)| *old == name).map_or(name, |(_, key)| key)
}

thread_local! {
    static PANELS: RefCell<Vec<Rc<Panel>>> = RefCell::default();
}

/// A panel built, by its id.
/// A bar's block's settings: a panel's (its name, icon and width), a widget's own (a module's, a KDL file's, its
/// plugin's); None for one without (the workspaces, the tray).
pub fn block_settings(name: &str) -> Option<crate::settings::Schema> {
    let name = match name {
        "status" => "panel.control",
        "clock" => "panel.calendar",
        "layout" => "widget.keymap",
        n => n,
    };
    if let Some(id) = name.strip_prefix("panel.") {
        let spec = Spec::of(id);
        let cols = panel(id).map_or_else(|| cols_of(&spec), |p| p.cols.get());
        return Some(crate::settings::panel_schema(id, &spec.name, &spec.icon, Some(cols)));
    }
    let w = name.strip_prefix("widget.")?;
    let plugin = w.strip_prefix("plugin.").and_then(|r| r.split('.').next()).map(|p| format!("plugin.{p}"));
    crate::settings::entry(&format!("widget.{w}")).or_else(|| plugin.and_then(|p| crate::settings::entry(&p))).map(|e| e.schema)
}

pub fn panel(id: &str) -> Option<Rc<Panel>> {
    PANELS.with(|p| p.borrow().iter().find(|p| p.spec.id == id).cloned())
}

/// The panel a widget is on, the control centre's first.
pub fn with_widget(key: &str) -> Option<Rc<Panel>> {
    PANELS.with(|p| {
        let ps = p.borrow();
        ps.iter().find(|p| p.spec.id == "control" && p.has(key)).or_else(|| ps.iter().find(|p| p.has(key))).cloned()
    })
}

/// The panel spec unrolled from tab in the bar, on the side of the bar it is on; its face put into face.
pub fn build(host: &Rc<crate::popup::Host>, hub: &Rc<Hub>, tab: &impl IsA<gtk4::Widget>, side: Side, spec: Spec, face: &gtk4::Box) -> Rc<Panel> {
    let col = gtk4::Box::new(Orientation::Vertical, 0);
    col.add_css_class("surface");
    let pages = gtk4::Stack::new();
    pages.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);
    pages.set_transition_duration(120);
    pages.set_vhomogeneous(false);
    // as wide as the page shown: the grid's cells, not the widest page (Appearance's chips)
    pages.set_hhomogeneous(false);
    pages.set_interpolate_size(true);
    col.append(&pages);

    let page = gtk4::Box::new(Orientation::Vertical, 10);
    let dims = crate::look::density(&crate::config::load().appearance);
    let grid = gtk4::Grid::new();
    grid.add_css_class("cc");
    grid.set_column_homogeneous(true);
    grid.set_column_spacing(dims.1 as u32);
    grid.set_row_spacing(dims.1 as u32);
    // the grid and the gallery scrolled within the screen's height, the footer under them always in sight
    let body = gtk4::Box::new(Orientation::Vertical, 10);
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    // as wide as the panel's cells (the popup's width), not the widest tile's or gallery row's natural width
    scroll.set_propagate_natural_width(false);
    scroll.set_child(Some(&body));
    page.append(&scroll);
    body.append(&grid);

    let gallery = gtk4::Box::new(Orientation::Vertical, 2);
    gallery.add_css_class("menu");
    gallery.set_visible(false);
    let inspector = gtk4::Box::new(Orientation::Vertical, 6);
    inspector.add_css_class("menu");
    inspector.set_visible(false);
    let shelf = gtk4::Box::new(Orientation::Vertical, 6);
    shelf.set_margin_start(6);
    shelf.set_margin_end(6);
    shelf.set_visible(false);
    body.append(&shelf);
    // the inspector as wide as the grid is: what it holds wraps or is cut, the panel never widened by it
    let fit = gtk4::ScrolledWindow::new();
    fit.set_policy(gtk4::PolicyType::External, gtk4::PolicyType::Never);
    fit.set_propagate_natural_height(true);
    fit.set_child(Some(&inspector));
    body.append(&fit);
    body.append(&gallery);

    // the footer: the control centre's gear to the Settings and palette to the Appearance, Edit
    let foot = gtk4::Box::new(Orientation::Horizontal, 4);
    let to_pages: &[(&str, &str, &str)] = if spec.pages {
        &[("emblem-system-symbolic", "Settings", "settings"), ("preferences-desktop-appearance-symbolic", "Appearance", "appearance")]
    } else {
        &[]
    };
    for &(icon, tip, to) in to_pages {
        let b = gtk4::Button::from_icon_name(icon);
        b.add_css_class("flat-round");
        b.set_tooltip_text(Some(t(tip)));
        let pg = pages.clone();
        b.connect_clicked(move |_| pg.set_visible_child_name(to));
        foot.append(&b);
    }
    let edit_button = crate::ui::chip(t("Edit"));
    edit_button.set_hexpand(true);
    edit_button.set_halign(Align::End);
    edit_button.set_valign(Align::Center);
    foot.append(&edit_button);
    page.append(&foot);
    pages.add_named(&page, Some("grid"));

    let settings = spec.pages.then(|| {
        let (pg, pg2) = (pages.clone(), pages.clone());
        // the Appearance and the bar's entries their own pages, not forms
        let settings = crate::settings::form::Page::new(
            move || pg.set_visible_child_name("grid"),
            move |id| match id {
                "appearance" => {
                    pg2.set_visible_child_name(id);
                    true
                }
                // the bar is edited in the bar itself
                "bar" => {
                    if let Err(e) = crate::bar::edit::start() {
                        eprintln!("ostrov: bar: {e}");
                    }
                    true
                }
                _ => false,
            },
        );
        pages.add_named(&settings.root, Some("settings"));
        let pg = pages.clone();
        pages.add_named(&appearance::page(move || pg.set_visible_child_name("grid")), Some("appearance"));
        let pg = pages.clone();
        pages.add_named(&crate::ui::gallery(move || pg.set_visible_child_name("grid")), Some("kit"));
        settings
    });

    let cols = cols_of(&spec);
    let popup = Popup::new(host, tab, side, spec.width_for(cols), &col);
    let reg = registry();
    let (base, hidden, shows) = load(&reg, &spec);
    let spec_id = spec.id.clone();
    let mut items = base.clone();
    grid::rescale(&mut items, BASE, cols);
    let face_icon = gtk4::Image::from_icon_name(&spec.icon);
    face_icon.set_tooltip_text(Some(&spec.name));
    let p = Rc::new(Panel {
        popup: popup.clone(),
        spec,
        reg,
        face: face.clone(),
        face_icon,
        shows: RefCell::new(shows),
        aligns: RefCell::new(aligns_of(&spec_id)),
        lively: RefCell::new(while_active_of(&spec_id)),
        lively_shown: RefCell::default(),
        pages,
        settings,
        dims: Cell::new(dims),
        grid,
        gallery,
        shelf,
        inspector,
        picked: RefCell::default(),
        edit_button: edit_button.clone(),
        items: RefCell::new(items),
        cols: Cell::new(cols),
        base: RefCell::new(base),
        hidden: RefCell::new(hidden),
        tiles: RefCell::default(),
        open: RefCell::default(),
        editing: Cell::new(false),
        state: Rc::new(RefCell::new(Value::Null)),
    });
    let keys: Vec<String> = p.items.borrow().iter().chain(p.hidden.borrow().iter()).map(|i| i.key.clone()).collect();
    for k in keys {
        if let Some(t) = p.tile(&k) {
            p.tiles.borrow_mut().insert(k, t);
        }
    }
    p.layout();
    p.faces();
    p.drags();
    PANELS.with(|ps| ps.borrow_mut().push(p.clone()));
    // its width set in a form (the bar's editor's, Settings') taken at once
    let w = Rc::downgrade(&p);
    crate::style::on_config(move || {
        let Some(p) = w.upgrade() else { return };
        let set = crate::config::load().panels.get(&p.spec.id).and_then(|c| c.cols).map(|n| n.clamp(MIN_COLS, MAX_COLS));
        if let Some(n) = set.filter(|n| *n != p.cols.get()) {
            p.set_cols(n);
        }
    });

    let me = Rc::downgrade(&p);
    edit_button.connect_clicked(move |_| {
        if let Some(p) = me.upgrade() {
            p.set_editing(!p.editing.get());
        }
    });
    // every opening at the grid, the menus folded, out of the editing (saved as it was left), drawn as things
    // are now: Keep Awake flips from outside the state too
    let me = Rc::downgrade(&p);
    // as tall as the screen lets it be, under the bar, its edges and footer
    let (sc, pw) = (scroll.clone(), Rc::downgrade(&popup));
    popup.on_open(move || {
        if let Some(h) = pw.upgrade().and_then(|p| p.host()) {
            sc.set_max_content_height((h.win.height() - crate::popup::bar() - 110).max(200));
            // no ring round what has the focus until a key moves it
            h.win.set_focus_visible(false);
        }
    });
    popup.on_open(move || {
        if let Some(p) = me.upgrade() {
            p.pages.set_visible_child_full("grid", gtk4::StackTransitionType::None);
            if let Some(s) = &p.settings {
                s.show(None);
            }
            p.set_editing(false);
            p.draw();
        }
    });
    let me = Rc::downgrade(&p);
    hub.on(move |v| {
        if let Some(p) = me.upgrade() {
            *p.state.borrow_mut() = v.clone();
            p.draw();
        }
    });
    let me = Rc::downgrade(&p);
    crate::style::on_config(move || {
        if let Some(p) = me.upgrade() {
            p.fit_density();
        }
    });
    p
}

#[cfg(test)]
mod tests {
    use super::toward;

    const TOGGLE: &[(u8, u8)] = &[(4, 1), (2, 1), (1, 1), (8, 1)];

    #[test]
    fn a_cell_as_wide_on_any_grid() {
        let spec = super::Spec { cell: 48.75, fixed: false, ..super::Spec::of("nope") };
        assert_eq!(spec.width_for(8), 390);
        // the padding less a gap (22) taken out, a cell 46 wide however many
        for cols in 4..=12u8 {
            assert!(((spec.width_for(cols) - 22) as f64 / cols as f64 - 46.0).abs() < 0.2, "{cols}");
        }
    }

    #[test]
    fn a_corner_takes_the_size_it_heads_for() {
        assert_eq!(toward(TOGGLE, (4, 1), (0.3, 0.0)), (4, 1));
        assert_eq!(toward(TOGGLE, (4, 1), (1.7, 0.0)), (8, 1));
        assert_eq!(toward(TOGGLE, (4, 1), (-1.0, 0.0)), (2, 1));
        assert_eq!(toward(TOGGLE, (4, 1), (-2.8, 0.0)), (1, 1));
        // down, for a widget that has a taller size
        assert_eq!(toward(&[(4, 2), (8, 2), (4, 4)], (4, 2), (0.0, 1.0)), (4, 4));
        // a slider has one size
        assert_eq!(toward(&[(8, 1)], (8, 1), (-3.0, 1.0)), (8, 1));
    }
}
