//! The control centre: the quick settings as a grid of widgets 8 cells wide (grid.rs), unrolled from the bar's
//! status. Each widget is one of the registry's (widgets.rs), in one of the sizes it allows: a toggle from a
//! square to the whole width, a slider the whole width alone. Its menu, if it has one, unfolds under its rows
//! across the whole width, one menu at a time, sliding open in 100 ms.
//!
//! "Edit" turns the grid to its editing: a widget dragged goes where it is dropped, the ones in its way pushed
//! down; dragged by its corner it takes the size nearest the pointer of those it allows; its minus takes it off;
//! the gallery under the grid puts back any widget not on it. The layout is kept in ~/.config/ostrov/panel.toml.
//!
//! Under the grid, beside Edit, a gear and a palette: the Settings page (settings/: a form for each of ostrov's
//! sections, each widget with a schema, each plugin's) and the Appearance page (appearance.rs), each sliding in
//! over the grid in the same popup, its arrow back at its top. The rows are as tall as [appearance]'s density.

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
pub mod grid;

use grid::{Item, COLS};

/// A widget made: its tile, its menu's card, how it draws the state and fits a size.
pub struct Widget {
    pub root: gtk4::Widget,
    pub menu: Option<gtk4::Widget>,
    pub draw: Box<dyn Fn(&Value)>,
    pub size: Box<dyn Fn(u8, u8)>,
}

impl Widget {
    /// A widget with nothing to fit to its size.
    pub fn new(root: &impl IsA<gtk4::Widget>, menu: Option<&gtk4::Box>, draw: impl Fn(&Value) + 'static) -> Widget {
        Widget {
            root: root.clone().upcast(),
            menu: menu.map(|m| m.clone().upcast()),
            draw: Box::new(draw),
            size: Box::new(|_, _| ()),
        }
    }

    /// A toggle's widget: drawn by draw, fitted as the toggle fits.
    pub fn toggle(t: &crate::ui::Toggle, menu: Option<&gtk4::Box>, draw: impl Fn(&Value) + 'static) -> Widget {
        let t2 = t.clone();
        Widget { size: Box::new(move |w, h| t2.size(w, h)), ..Widget::new(&t.root, menu, draw) }
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
}

/// A widget placed on the grid, as panel.toml keeps it.
#[derive(Serialize, Deserialize)]
struct Placed {
    id: String,
    x: u8,
    y: u8,
    w: u8,
    h: u8,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    widget: Vec<Placed>,
}

fn layout_path() -> std::path::PathBuf {
    crate::hub::home().join(".config/ostrov/panel.toml")
}

/// The size of those allowed nearest (w, h), a row off counting as two columns.
fn nearest(sizes: &[(u8, u8)], w: i32, h: i32) -> (u8, u8) {
    let d = |&(sw, sh): &(u8, u8)| (sw as i32 - w).pow(2) + 4 * (sh as i32 - h).pow(2);
    sizes.iter().copied().min_by_key(d).unwrap_or((COLS, 1))
}

/// The layout as panel.toml has it (widgets unknown dropped, sizes not allowed made the nearest allowed), else
/// the one that came before the grid: the battery and the buttons, the sliders, the toggles two a row.
fn load(reg: &[Meta]) -> Vec<Item> {
    let saved: Option<Saved> = std::fs::read_to_string(layout_path()).ok().and_then(|t| {
        toml::from_str(&t).map_err(|e| eprintln!("ostrov: {}: {e}", layout_path().display())).ok()
    });
    let mut items: Vec<Item> = match saved {
        Some(s) => s
            .widget
            .into_iter()
            .filter_map(|p| {
                let m = reg.iter().find(|m| m.id == p.id)?;
                let (w, h) = nearest(m.sizes, p.w as i32, p.h as i32);
                Some(Item { key: p.id, x: p.x, y: p.y, w, h })
            })
            .collect(),
        None => {
            let it = |key: &str, x, y, w, h| Item { key: key.into(), x, y, w, h };
            vec![
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
                it("openvpn", 0, 6, 4, 1),
                it("vless", 4, 6, 4, 1),
                it("awake", 0, 7, 4, 1),
                it("headset", 4, 7, 4, 1),
                it("displays", 0, 8, 4, 1),
            ]
        }
    };
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.key.clone()));
    grid::compact(&mut items);
    items
}

fn save(items: &[Item]) {
    let s = Saved {
        widget: items.iter().map(|i| Placed { id: i.key.clone(), x: i.x, y: i.y, w: i.w, h: i.h }).collect(),
    };
    let text = match toml::to_string(&s) {
        Ok(t) => format!("# ostrov's control centre, as its Edit leaves it\n\n{t}"),
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
/// the drag started from, the cells it was last put at.
type Dragged = (Item, bool, Vec<Item>, (u8, u8, u8, u8));

pub struct Panel {
    pub popup: Rc<Popup>,
    reg: Vec<Meta>,
    /// the grid's page, the Settings', the Appearance's
    pages: gtk4::Stack,
    settings: Rc<crate::settings::form::Page>,
    /// a row's height and the gap between cells, in pixels, as the density says
    dims: Cell<(i32, i32)>,
    grid: gtk4::Grid,
    gallery: gtk4::Box,
    edit_button: gtk4::Button,
    items: RefCell<Vec<Item>>,
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
        let key = match name {
            "system" => "session",
            "outs" => "volume",
            "ins" => "mic",
            "night" => "brightness",
            "theme" => "wallpaper",
            n => n,
        };
        self.set_open(key);
    }

    /// Open at a page: "settings" (at an entry's form, if given), "appearance", "grid".
    pub fn open_page(&self, page: &str, entry: Option<&str>) {
        if !self.popup.is_open() {
            self.popup.open();
        }
        if page == "settings" {
            self.settings.show(entry);
        }
        if !(page == "settings" && entry == Some("appearance")) {
            self.pages.set_visible_child_full(page, gtk4::StackTransitionType::None);
        }
    }

    /// The rows as the density in the config says, laid out again if that changed.
    fn fit_density(&self) {
        let (row, gap) = crate::look::density(&crate::config::load().appearance.density);
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
        wrap.set_child(Some(&widget.root));
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
        for w in [remove.upcast_ref::<gtk4::Widget>(), grip.upcast_ref()] {
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
        for it in items.iter() {
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
    }

    /// In the editing or out of it, the layout saved on the way out.
    fn set_editing(self: &Rc<Self>, on: bool) {
        if !on && self.editing.get() {
            save(&self.items.borrow());
        }
        self.editing.set(on);
        self.set_open("");
        self.edit_button.set_label(if on { "Done" } else { "Edit" });
        for t in self.tiles.borrow().values() {
            t.widget.root.set_can_target(!on);
            let mut c = t.widget.root.next_sibling();
            while let Some(w) = c {
                w.set_visible(on);
                c = w.next_sibling();
            }
        }
        if on {
            self.grid.add_css_class("editing");
        } else {
            self.grid.remove_css_class("editing");
        }
        self.fill_gallery();
        self.gallery.set_visible(on);
    }

    fn remove(self: &Rc<Self>, key: &str) {
        self.items.borrow_mut().retain(|i| i.key != key);
        grid::compact(&mut self.items.borrow_mut());
        self.tiles.borrow_mut().remove(key);
        self.layout();
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
        self.fill_gallery();
        self.draw();
    }

    /// The gallery: every widget not on the grid, its sizes, a click putting it on.
    fn fill_gallery(self: &Rc<Self>) {
        clear(&self.gallery);
        self.gallery.append(&label("Add Widgets", "title"));
        let on: Vec<String> = self.items.borrow().iter().map(|i| i.key.clone()).collect();
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

    /// Dragging a tile in the editing: by its body to move it, by its corner to size it. The grid's own drag,
    /// in the grid's coordinates, since the tile moves under the pointer as it goes.
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
            if let Some(t) = p.tiles.borrow().get(&it.key) {
                t.wrap.add_css_class("dragged");
            }
            let last = (it.x, it.y, it.w, it.h);
            *a.borrow_mut() = Some((it, corner, items, last));
        });
        let (me, a) = (Rc::downgrade(self), at.clone());
        drag.connect_drag_update(move |_, dx, dy| {
            let Some(p) = me.upgrade() else { return };
            let mut a = a.borrow_mut();
            let Some((it, corner, start, last)) = a.as_mut() else { return };
            let Some(m) = p.reg.iter().find(|m| m.id == it.key) else { return };
            let (row, gap) = p.dims.get();
            let cw = (p.grid.width() + gap) as f64 / COLS as f64;
            let (cx, cy) = ((dx / cw).round() as i32, (dy / (row + gap) as f64).round() as i32);
            let next = if *corner {
                let (w, h) = nearest(m.sizes, it.w as i32 + cx, it.h as i32 + cy);
                (it.x, it.y, w, h)
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
        });
        let (me, a) = (Rc::downgrade(self), at);
        drag.connect_drag_end(move |_, _, _| {
            let (Some(p), Some((it, ..))) = (me.upgrade(), a.borrow_mut().take()) else { return };
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

pub fn build(host: &Rc<crate::popup::Host>, hub: &Rc<Hub>, tab: &gtk4::Box) -> Rc<Panel> {
    let col = gtk4::Box::new(Orientation::Vertical, 0);
    col.add_css_class("surface");
    let pages = gtk4::Stack::new();
    pages.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);
    pages.set_transition_duration(120);
    pages.set_vhomogeneous(false);
    pages.set_interpolate_size(true);
    col.append(&pages);

    let page = gtk4::Box::new(Orientation::Vertical, 10);
    let dims = crate::look::density(&crate::config::load().appearance.density);
    let grid = gtk4::Grid::new();
    grid.add_css_class("cc");
    grid.set_column_homogeneous(true);
    grid.set_column_spacing(dims.1 as u32);
    grid.set_row_spacing(dims.1 as u32);
    page.append(&grid);

    let gallery = gtk4::Box::new(Orientation::Vertical, 2);
    gallery.add_css_class("menu");
    gallery.set_visible(false);
    page.append(&gallery);

    // the footer: the gear to the Settings, the palette to the Appearance, Edit
    let foot = gtk4::Box::new(Orientation::Horizontal, 4);
    for (icon, tip, to) in [
        ("emblem-system-symbolic", "Settings", "settings"),
        ("preferences-desktop-appearance-symbolic", "Appearance", "appearance"),
    ] {
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

    let (pg, pg2) = (pages.clone(), pages.clone());
    let settings = crate::settings::form::Page::new(
        move || pg.set_visible_child_name("grid"),
        move |id| id == "appearance" && { pg2.set_visible_child_name("appearance"); true },
    );
    pages.add_named(&settings.root, Some("settings"));
    let pg = pages.clone();
    pages.add_named(&appearance::page(move || pg.set_visible_child_name("grid")), Some("appearance"));

    let popup = Popup::new(host, tab, Side::Right, 390, &col);
    let mut reg: Vec<Meta> = crate::modules::ALL
        .iter()
        .flat_map(|m| m.widgets)
        .map(|d| Meta { id: d.id, name: d.name, icon: d.icon, sizes: d.sizes, make: Rc::new(d.make), settings: d.settings })
        .collect();
    reg.extend(crate::plugins::metas());
    for m in &reg {
        if let Some(schema) = m.settings {
            crate::settings::register(&format!("widget.{}", m.id), m.name, m.icon, schema());
        }
    }
    let items = load(&reg);
    let p = Rc::new(Panel {
        popup: popup.clone(),
        reg,
        pages,
        settings,
        dims: Cell::new(dims),
        grid,
        gallery,
        edit_button: edit_button.clone(),
        items: RefCell::new(items),
        tiles: RefCell::default(),
        open: RefCell::default(),
        editing: Cell::new(false),
        state: Rc::new(RefCell::new(Value::Null)),
    });
    let keys: Vec<String> = p.items.borrow().iter().map(|i| i.key.clone()).collect();
    for k in keys {
        if let Some(t) = p.tile(&k) {
            p.tiles.borrow_mut().insert(k, t);
        }
    }
    p.layout();
    p.drags();

    let me = Rc::downgrade(&p);
    edit_button.connect_clicked(move |_| {
        if let Some(p) = me.upgrade() {
            p.set_editing(!p.editing.get());
        }
    });
    // every opening at the grid, the menus folded, out of the editing (saved as it was left), drawn as things
    // are now: Keep Awake flips from outside the state too
    let me = Rc::downgrade(&p);
    popup.on_open(move || {
        if let Some(p) = me.upgrade() {
            p.pages.set_visible_child_full("grid", gtk4::StackTransitionType::None);
            p.settings.show(None);
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
