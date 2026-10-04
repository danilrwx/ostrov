//! The tray: StatusNotifierItem through the system-tray crate on a Tokio runtime of its own. A click activates
//! an item; a right click (or any, for an item that is only a menu) unrolls its DBusMenu out of its icon, a popup
//! like the others shared by all the icons. One client for every bar's tray.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, glib};
use system_tray::client::{ActivateRequest, Client};
use system_tray::item::StatusNotifierItem;
use system_tray::menu::{MenuItem, MenuType, ToggleState, ToggleType, TrayMenu};

use super::{pill, slot, Block, Ctx};
use crate::popup::{Popup, Side};

/// What the tray shows of an item: its address, its icon, its menu.
#[derive(Clone)]
struct Entry {
    address: String,
    item: StatusNotifierItem,
    menu: Option<TrayMenu>,
}

/// The tray in its own Tokio runtime: every change sends the whole list over; activations come back.
fn serve(tx: async_channel::Sender<Vec<Entry>>, mut rx: tokio::sync::mpsc::UnboundedReceiver<ActivateRequest>) {
    let Ok(rt) = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() else { return };
    rt.block_on(async move {
        let Ok(client) = Client::new().await else { return };
        let client = std::sync::Arc::new(client);
        let mut events = client.subscribe();
        let snapshot = |client: &Client| -> Vec<Entry> {
            let map = client.items();
            let map = map.lock().unwrap();
            let mut v: Vec<Entry> = map
                .iter()
                .map(|(a, (item, menu))| Entry { address: a.clone(), item: item.clone(), menu: menu.clone() })
                .collect();
            v.sort_by(|a, b| a.item.id.cmp(&b.item.id));
            v
        };
        let _ = tx.send(snapshot(&client)).await;
        let c2 = client.clone();
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let _ = c2.activate(req).await;
            }
        });
        while events.recv().await.is_ok() {
            let _ = tx.send(snapshot(&client)).await;
        }
    });
}

/// An item's icon: its theme icon by name, else its pixmap (ARGB32, big-endian) as a texture.
fn icon(item: &StatusNotifierItem) -> gtk4::Image {
    if let Some(name) = item.icon_name.as_deref().filter(|n| !n.is_empty()) {
        return gtk4::Image::from_icon_name(name);
    }
    if let Some(px) = item.icon_pixmap.as_ref().and_then(|v| v.iter().max_by_key(|p| p.width)) {
        let mut rgba = Vec::with_capacity(px.pixels.len());
        for c in px.pixels.chunks_exact(4) {
            rgba.extend_from_slice(&[c[1], c[2], c[3], c[0]]);
        }
        let tex = gdk::MemoryTexture::new(
            px.width,
            px.height,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(rgba),
            (px.width * 4) as usize,
        );
        return gtk4::Image::from_paintable(Some(&tex));
    }
    gtk4::Image::from_icon_name("application-x-executable-symbolic")
}

/// A menu's entries into a box of items, a submenu's entries indented under its own.
fn fill(
    bx: &gtk4::Box,
    items: &[MenuItem],
    depth: i32,
    at: (&str, &str),
    act: &tokio::sync::mpsc::UnboundedSender<ActivateRequest>,
    pop: &Rc<Popup>,
) {
    for it in items.iter().filter(|i| i.visible) {
        if matches!(it.menu_type, MenuType::Separator) {
            bx.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
            continue;
        }
        let toggles = !matches!(it.toggle_type, ToggleType::CannotBeToggled);
        let mark = match it.toggle_state {
            ToggleState::On if toggles => "■ ",
            ToggleState::Off if toggles => "□ ",
            _ => "",
        };
        let label = it.label.clone().unwrap_or_default().replace("__", "\u{0}").replace('_', "").replace('\u{0}', "_");
        let b = gtk4::Button::with_label(&format!("{mark}{label}"));
        b.add_css_class("item");
        b.set_sensitive(it.enabled);
        b.set_margin_start(depth * 12);
        if let Some(l) = b.child().and_downcast::<gtk4::Label>() {
            l.set_xalign(0.0);
        }
        if it.submenu.is_empty() {
            let (act, address, path, id, pop) = (act.clone(), at.0.to_string(), at.1.to_string(), it.id, pop.clone());
            b.connect_clicked(move |_| {
                let _ = act.send(ActivateRequest::MenuItem { address: address.clone(), menu_path: path.clone(), submenu_id: id });
                pop.close();
            });
            bx.append(&b);
        } else {
            b.set_sensitive(false);
            bx.append(&b);
            fill(bx, &it.submenu, depth + 1, at, act, pop);
        }
    }
}

/// The one tray client's, every bar's tray drawn from it: the items as last sent, where activations go, each bar's
/// tray's drawing (false once its bar is gone).
struct Shared {
    items: Vec<Entry>,
    act: tokio::sync::mpsc::UnboundedSender<ActivateRequest>,
    draws: Vec<Box<dyn Fn(&[Entry]) -> bool>>,
}

thread_local!(static SHARED: RefCell<Option<Shared>> = const { RefCell::new(None) });

/// The tray client started, its items drawn by every bar's tray as they change.
fn start() -> Shared {
    let (tx, rx) = async_channel::unbounded::<Vec<Entry>>();
    let (act, act_rx) = tokio::sync::mpsc::unbounded_channel::<ActivateRequest>();
    std::thread::spawn(move || serve(tx, act_rx));
    glib::spawn_future_local(async move {
        while let Ok(items) = rx.recv().await {
            SHARED.with(|s| {
                if let Some(s) = s.borrow_mut().as_mut() {
                    s.items = items;
                    s.draws.retain(|d| d(&s.items));
                }
            });
        }
    });
    Shared { items: vec![], act, draws: vec![] }
}

pub fn build(cx: &Rc<Ctx>) -> Block {
    let tray = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    body.add_css_class("surface");
    body.add_css_class("tray-menu");
    let pop = Popup::new(&cx.host, &tray, Side::Right, -1, &body);

    let act_tx = SHARED.with(|s| s.borrow_mut().get_or_insert_with(start).act.clone());
    let (t, p) = (tray.clone(), pop.clone());
    let draw = move |items: &[Entry]| {
        crate::style::clear(&t);
        for e in items.iter().cloned() {
            let cell = pill();
            cell.add_css_class("tray-item");
            cell.append(&icon(&e.item));
            cell.set_cursor_from_name(Some("pointer"));
            let cell_slot = slot(&cell);
            let click = gtk4::GestureClick::new();
            click.set_button(0);
            let (act, cs, pop, body) = (act_tx.clone(), cell_slot.clone(), p.clone(), body.clone());
            click.connect_released(move |g, _, _, _| {
                if g.current_button() != 3 && !e.item.item_is_menu {
                    let _ = act.send(ActivateRequest::Default { address: e.address.clone(), x: 0, y: 0 });
                    return;
                }
                let Some(menu) = &e.menu else { return };
                // the same icon again closes it
                if pop.is_open() && pop.tab() == cs.clone().upcast::<gtk4::Widget>() {
                    return pop.close();
                }
                crate::style::clear(&body);
                let path = e.item.menu.clone().unwrap_or_default();
                fill(&body, &menu.submenus, 0, (&e.address, &path), &act, &pop);
                pop.set_tab(&cs);
                pop.open();
            });
            cell.add_controller(click);
            t.append(&cell_slot);
        }
        // its bar's window gone with its monitor
        t.root().is_some()
    };
    SHARED.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            draw(&s.items);
            s.draws.push(Box::new(draw));
        }
    });
    Block { popup: Some(pop), ..Block::new(&tray) }
}
