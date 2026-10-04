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
use crate::popup::{Popup, Side};
use crate::style::{clear, label};

mod appearance;
mod bar_editor;
pub mod grid;

use grid::{Item, COLS};

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
}

impl Face {
    pub fn new(root: &impl IsA<gtk4::Widget>) -> Face {
        Face { root: root.clone().upcast(), active: Rc::new(Cell::new(true)) }
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
            face: Some(Face { root: t.face.clone().upcast(), active: t.active.clone() }),
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
                    name: d.name,
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
    ("night", "brightness"),
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
}

impl Spec {
    /// The panel id: the control centre, the calendar, or one of [panels.ID] (empty until edited).
    pub fn of(id: &str) -> Spec {
        let it = |key: &str, x, y, w, h| Item { key: key.into(), x, y, w, h };
        let spec = |name: &str, icon: &str, width, pages, layout| Spec {
            id: id.into(),
            name: name.into(),
            icon: icon.into(),
            width,
            pages,
            layout,
        };
        let cfg = crate::config::load().panels.remove(id);
        match id {
            "control" => spec("Control Centre", "emblem-system-symbolic", 390, true, vec![
                it("battery", 0, 0, 5, 1),
                it("screenshot", 5, 0, 1, 1),
                it("lock", 6, 0, 1, 1),
                it("session", 7, 0, 1, 1),
                it("volume", 0, 1, 8, 1),
                it("mic", 0, 2, 8, 1),
                it("brightness", 0, 3, 8, 1),
                it("wifi", 0, 4, 4, 1),
                it("bt", 4, 4, 4, 1),
                it("power", 0, 5, 4, 1),
                it("wallpaper", 4, 5, 4, 1),
                it("awake", 0, 6, 4, 1),
                it("headset", 4, 6, 4, 1),
                it("displays", 0, 7, 4, 1),
            ]),
            // the calendar as it was: the weather and the player over the notifications at the left; the
            // date, the month, the coming events at the right
            "calendar" => spec("Calendar", "x-office-calendar-symbolic", 680, false, vec![
                it("weather", 0, 0, 4, 2),
                it("clock", 4, 0, 4, 1),
                it("month", 4, 1, 4, 5),
                it("media", 0, 2, 4, 2),
                it("notifications", 0, 4, 4, 6),
                it("agenda", 4, 6, 4, 4),
            ]),
            _ => spec(id, "view-grid-symbolic", 390, false, Vec::new()),
        }
        .with(cfg)
    }

    fn with(mut self, cfg: Option<crate::config::PanelSpec>) -> Spec {
        if let Some(c) = cfg {
            self.name = c.name.unwrap_or(self.name);
            self.icon = c.icon.unwrap_or(self.icon);
            self.width = c.width.unwrap_or(self.width);
        }
        self
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
    sizes.iter().copied().min_by_key(d).unwrap_or((COLS, 1))
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

/// A panel's layout as panel.toml has it (widgets unknown dropped, sizes not allowed made the nearest allowed),
/// else its spec's; the widgets in the bar alone, where they were; and when its badges show where it says.
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
        None => spec.layout.iter().filter(|i| reg.iter().any(|m| m.id == i.key)).cloned().collect(),
    };
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.key.clone()));
    hidden.retain(|i| seen.insert(i.key.clone()));
    grid::settle(&mut items);
    (items, hidden, shows)
}

/// A panel's layout into panel.toml, the other panels' kept.
fn save(id: &str, items: &[Item], hidden: &[Item], shows: &HashMap<String, Show>) {
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
    /// its face in the bar: its widgets' badges, else its icon
    face: gtk4::Box,
    face_icon: gtk4::Image,
    shows: RefCell<HashMap<String, Show>>,
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
        self.popup.open();
        if name == "edit" {
            return self.set_editing(true);
        }
        self.set_open(alias(name));
    }

    /// A widget on the grid made w×h, a size it allows, saved.
    pub fn resize(self: &Rc<Self>, key: &str, w: u8, h: u8) -> Result<(), String> {
        let m = self.reg.iter().find(|m| m.id == key).ok_or(format!("no widget {key}"))?;
        if !m.sizes.contains(&(w, h)) {
            let all: Vec<String> = m.sizes.iter().map(|(w, h)| format!("{w}×{h}")).collect();
            return Err(format!("{key} is {}", all.join(", ")));
        }
        let it = self.items.borrow().iter().find(|i| i.key == key).cloned().ok_or(format!("{key} is not on the grid"))?;
        let mut items = self.items.borrow().clone();
        grid::place(&mut items, key, it.x, it.y, w, h);
        *self.items.borrow_mut() = items;
        self.layout();
        self.faces();
        save(&self.spec.id, &self.items.borrow(), &self.hidden.borrow(), &self.shows.borrow());
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

        let wrap = gtk4::Overlay::new();
        wrap.add_css_class("tile");
        wrap.set_widget_name(key);
        // the tile as tall as its cells and no taller: what it holds clipped to them (scrolled, a scroll of its
        // own aside), never a row pushed taller than its neighbours
        let clip = gtk4::ScrolledWindow::new();
        clip.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
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
    fn layout(&self) {
        while let Some(c) = self.grid.first_child() {
            self.grid.remove(&c);
        }
        let items = self.items.borrow();
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
                (t.widget.size)(it.w, it.h);
            }
        }
        for (b, bx) in &bands {
            fit_band(bx.upcast_ref());
            self.grid.attach(bx, 0, *b as i32 + above(*b, false), COLS as i32, 1);
        }
    }

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
    fn set_editing(self: &Rc<Self>, on: bool) {
        if !on && self.editing.get() {
            save(&self.spec.id, &self.items.borrow(), &self.hidden.borrow(), &self.shows.borrow());
        }
        self.editing.set(on);
        self.set_open("");
        self.edit_button.set_label(if on { "Done" } else { "Edit" });
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
    }

    fn remove(self: &Rc<Self>, key: &str) {
        if *self.picked.borrow() == key {
            self.pick("");
        }
        self.items.borrow_mut().retain(|i| i.key != key);
        self.hidden.borrow_mut().retain(|i| i.key != key);
        grid::compact(&mut self.items.borrow_mut());
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

        let line = |title: &str, w: &gtk4::Widget| {
            let bx = gtk4::Box::new(Orientation::Horizontal, 8);
            let l = label(title, "dim");
            l.set_width_chars(10);
            bx.append(&l);
            bx.append(w);
            self.inspector.append(&bx);
        };
        // its sizes, the one it has pressed
        if m.sizes.len() > 1 && !off {
            let names: Vec<String> = m.sizes.iter().map(|(w, h)| format!("{w}×{h}")).collect();
            let strs: Vec<&str> = names.iter().map(String::as_str).collect();
            let at = m.sizes.iter().position(|&s| s == (it.w, it.h));
            let (me, k, sizes) = (Rc::downgrade(self), key.clone(), m.sizes);
            let chips = crate::ui::chips(&strs, at, move |i| {
                let Some(p) = me.upgrade() else { return };
                let Some(it) = p.items.borrow().iter().find(|x| x.key == k).cloned() else { return };
                let (w, h) = sizes[i];
                let mut items = p.items.borrow().clone();
                grid::place(&mut items, &k, it.x, it.y, w, h);
                *p.items.borrow_mut() = items;
                p.layout();
                p.faces();
                p.inspect();
            });
            chips.set_margin_start(0);
            line("Size", chips.upcast_ref());
        }
        // when its badge is in the bar, and the bar alone
        if self.tiles.borrow().get(&key).is_some_and(|t| t.widget.face.is_some()) {
            let now = self.show(&key, m.bar);
            let names: Vec<&str> = Show::ALL.iter().map(|s| s.1).collect();
            let at = Show::ALL.iter().position(|s| s.0 == now);
            let (me, k) = (Rc::downgrade(self), key.clone());
            let chips = crate::ui::chips(&names, at, move |i| {
                if let Some(p) = me.upgrade() {
                    p.shows.borrow_mut().insert(k.clone(), Show::ALL[i].0);
                    p.faces();
                    p.inspect();
                }
            });
            chips.set_margin_start(0);
            line("In the bar", chips.upcast_ref());
        }
        let acts = gtk4::Box::new(Orientation::Horizontal, 6);
        let act = |text: &str, f: Box<dyn Fn(&Rc<Panel>)>| {
            let b = gtk4::Button::with_label(text);
            b.add_css_class("chip");
            let me = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(p) = me.upgrade() {
                    f(&p);
                }
            });
            acts.append(&b);
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
            self.inspector.append(&label("Settings", "dim"));
            self.inspector.append(&crate::settings::form::form(&e.schema));
        }
    }

    /// A widget's tile off the panel, its badge left in the bar (shown always, if it was never).
    fn hide(self: &Rc<Self>, key: &str) {
        let Some(it) = self.items.borrow().iter().find(|i| i.key == key).cloned() else { return };
        self.pick("");
        self.items.borrow_mut().retain(|i| i.key != key);
        grid::compact(&mut self.items.borrow_mut());
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
        (it.x, it.y) = grid::free(&self.items.borrow(), it.w, it.h);
        self.items.borrow_mut().push(it);
        self.layout();
        self.faces();
        self.fill_gallery();
    }

    /// A widget put on the grid at its default size, in the first place it fits.
    fn add(self: &Rc<Self>, key: &str) {
        let Some(m) = self.reg.iter().find(|m| m.id == key) else { return };
        let Some(t) = self.tile(key) else { return };
        let (w, h) = m.sizes.first().copied().unwrap_or((COLS, 1));
        let (x, y) = grid::free(&self.items.borrow(), w, h);
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
        self.shelf.append(&label("In the bar only", "dim"));
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
        self.gallery.append(&label("Add Widgets", "title"));
        let mut on: Vec<String> = self.items.borrow().iter().map(|i| i.key.clone()).collect();
        on.extend(hidden);
        let mut any = false;
        for m in self.reg.iter().filter(|m| !on.iter().any(|k| k == m.id)) {
            any = true;
            let sizes: Vec<String> = m.sizes.iter().map(|(w, h)| format!("{w}×{h}")).collect();
            let me = Rc::downgrade(self);
            let id = m.id;
            self.gallery.append(&crate::ui::row(m.icon, m.name, &sizes.join(" "), false, move || {
                if let Some(p) = me.upgrade() {
                    p.add(id);
                }
            }));
        }
        if !any {
            self.gallery.append(&label("Every widget is on the panel", "dim"));
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
            let mut pitch = ((p.grid.width() + gap) as f64 / COLS as f64, (row + gap) as f64);
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
                let (nw, nh) = toward(m.sizes, (it.w, it.h), (fx, fy));
                (it.x, it.y, nw, nh)
            } else {
                let x = (it.x as i32 + cx).clamp(0, (COLS - it.w) as i32) as u8;
                (x, (it.y as i32 + cy).max(0) as u8, it.w, it.h)
            };
            if next == *last {
                return;
            }
            *last = next;
            let mut items = start.clone();
            grid::place(&mut items, &it.key, next.0, next.1, next.2, next.3);
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

/// A widget by the name its menu had in the panel before the grid (ostrov menu night), or by its own.
pub fn alias(name: &str) -> &str {
    ALIASES.iter().find(|(old, _)| *old == name).map_or(name, |(_, key)| key)
}

thread_local! {
    static PANELS: RefCell<Vec<Rc<Panel>>> = RefCell::default();
}

/// A panel built, by its id.
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
    scroll.set_propagate_natural_width(true);
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
    body.append(&inspector);
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
        b.set_tooltip_text(Some(tip));
        let pg = pages.clone();
        b.connect_clicked(move |_| pg.set_visible_child_name(to));
        foot.append(&b);
    }
    let edit_button = gtk4::Button::with_label("Edit");
    edit_button.add_css_class("chip");
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
            move |id| (id == "appearance" || id == "bar") && { pg2.set_visible_child_name(id); true },
        );
        pages.add_named(&settings.root, Some("settings"));
        let pg = pages.clone();
        pages.add_named(&appearance::page(move || pg.set_visible_child_name("grid")), Some("appearance"));
        let pg = pages.clone();
        pages.add_named(&bar_editor::page(move || pg.set_visible_child_name("settings")), Some("bar"));
        settings
    });

    let popup = Popup::new(host, tab, side, spec.width, &col);
    let reg = registry();
    let (items, hidden, shows) = load(&reg, &spec);
    let face_icon = gtk4::Image::from_icon_name(&spec.icon);
    face_icon.set_tooltip_text(Some(&spec.name));
    let p = Rc::new(Panel {
        popup: popup.clone(),
        spec,
        reg,
        face: face.clone(),
        face_icon,
        shows: RefCell::new(shows),
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
    let (sc, h) = (scroll.clone(), Rc::downgrade(host));
    popup.on_open(move || {
        if let Some(h) = h.upgrade() {
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
