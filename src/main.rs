//! ostrov ("island" in Russian): the desktop's shell, everything in one place, in Rust on GTK4 and
//! gtk4-layer-shell, in place of the Quickshell bar (~/dotfiles/config/quickshell). The bar: Hyprland's
//! workspaces as dots, the clock and the weather in the middle, at the right the layout, the tray
//! (StatusNotifierItem with its DBusMenu) and the status (Wi-Fi, volume, battery); out of its blocks the quick
//! settings and the calendar; the notifications. The state from wmd watch, the workspaces from Hyprland's
//! sockets, the tray from the system-tray crate.

mod calendar;
mod hub;
mod launcher;
mod lock;
mod notes;
mod panel;
mod popup;
mod wm;


use hub::{home, Hub};

use gtk4::prelude::*;
use gtk4::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use system_tray::client::{ActivateRequest, Client};
use system_tray::item::StatusNotifierItem;
use system_tray::menu::{MenuItem, MenuType, ToggleState, ToggleType, TrayMenu};

const CSS: &str = r#"
/* the colours, set here alone: the panels' ground (the panels, the calendar, the tab, the tray's menus, toasts,
   the OSD) and a hovered thing's */
@define-color panel rgba(56, 56, 56, 0.88);
@define-color hover rgba(255, 255, 255, 0.15);
window { background: rgba(0, 0, 0, ALPHA); }
window.panel-window { background: transparent; }
/* the panel and the bar's tab it grows out of: the bar's hover (white at 15% over black), nearly solid so the
   panel reads over whatever lies under it */
.panel { background: @panel; border: 1px solid transparent; border-radius: 10px; padding: 14px; }
.panel.attached { border-top: none; border-radius: 0 0 10px 10px; padding-top: 4px; }
.edge { background: @panel; border-top: 1px solid transparent; border-left: 1px solid transparent; border-top-left-radius: 10px; min-height: 10px; }
.gap { background: @panel; border-right: 1px solid transparent; min-height: 10px; }
.gap-mid { background: @panel; min-height: 10px; }
.edge-right { background: @panel; border-top: 1px solid transparent; border-right: 1px solid transparent; border-top-right-radius: 10px; min-height: 10px; }
.panel label { font-size: 10pt; }
.bold { font-weight: bold; }
.dim { color: #888888; }
.error { color: #cd0000; }
.title { font-weight: bold; font-size: 11pt; }
.battery { background: rgba(255, 255, 255, 0.08); border-radius: 6px; padding: 0 14px; min-height: 40px; }
button { background: none; border: none; box-shadow: none; outline: none; min-height: 0; min-width: 0; padding: 0; }
button.round { background: rgba(255, 255, 255, 0.08); border: 1px solid #333333; border-radius: 6px; min-width: 40px; min-height: 40px; }
button.round:hover, button.arrow:hover, button.flat-round:hover { background: @hover; }
button.round.open { background: #ffffff; }
button.round.open image { color: #000000; }
button.arrow, button.flat-round { border-radius: 6px; min-width: 32px; min-height: 32px; }
button.arrow image { transition: -gtk-icon-transform 100ms; }
button.arrow.open { background: #ffffff; }
button.arrow.open image { color: #000000; -gtk-icon-transform: rotate(90deg); }
scale { padding: 0 4px; }
scale trough { min-height: 14px; border-radius: 6px; background: rgba(255, 255, 255, 0.1); }
scale trough highlight { border-radius: 6px; background: #ffffff; border: none; margin: 0; min-height: 14px; min-width: 0; }
scale slider { min-width: 0; min-height: 0; margin: 0; background: none; box-shadow: none; border: none; }
.toggle { background: rgba(255, 255, 255, 0.08); border: 1px solid #333333; border-radius: 6px; min-height: 48px; }
.toggle.on { background: #ffffff; border-color: #ffffff; }
.toggle.on label, .toggle.on image { color: #000000; }
.toggle.on .toggle-sub { color: #333333; }
.toggle-main { padding: 0 6px 0 14px; border-radius: 6px; }
.toggle-main:hover { background: rgba(255, 255, 255, 0.08); }
.toggle-title { font-weight: bold; }
.toggle-sub { color: #888888; font-size: 8.5pt; }
button.toggle-side { min-width: 40px; border-left: 1px solid #333333; border-radius: 0 6px 6px 0; }
.toggle.on button.toggle-side { border-left-color: #999999; }
.toggle.on button.toggle-side.open { background: #d0d0d0; }
.menu { background: rgba(255, 255, 255, 0.06); border: 1px solid #333333; border-radius: 6px; padding: 10px; margin-top: 4px; }
.menu-head { margin-bottom: 6px; }
.badge { background: #ffffff; color: #000000; border-radius: 6px; min-width: 32px; min-height: 32px; }
button.item { padding: 0 10px; min-height: 34px; border-radius: 6px; }
button.item:hover { background: @hover; }
button.item.on { background: #ffffff; }
button.item.on label, button.item.on image { color: #000000; }
button.connect { background: #ffffff; padding: 0 10px; border-radius: 6px; min-height: 30px; }
button.connect label { color: #000000; font-weight: bold; }
entry, passwordentry { background: rgba(0, 0, 0, 0.2); border: 1px solid #333333; border-radius: 6px; min-height: 30px; padding: 0 8px; }
entry:focus-within, passwordentry:focus-within { border-color: #ffffff; }
separator { background: #333333; margin: 4px 4px; min-height: 1px; min-width: 1px; }
.card { background: rgba(255, 255, 255, 0.06); border: 1px solid #333333; border-radius: 6px; padding: 10px; }
.card.critical { border-color: #cd0000; }
.toasts > .card { background: @panel; border-color: #ffffff; border-radius: 10px; }
.osd { background: @panel; border: 1px solid #ffffff; border-radius: 10px; padding: 12px 16px; }
.art { border-radius: 6px; }
.date { font-size: 14pt; font-weight: bold; }
button.chip, togglebutton.chip, button.chip:checked { background: rgba(255, 255, 255, 0.08); border: 1px solid #333333; border-radius: 6px; padding: 4px 10px; min-height: 0; }
button.chip:hover { background: @hover; }
button.chip:checked { background: #ffffff; }
button.chip:checked label { color: #000000; }
progressbar.progress trough { min-height: 3px; border-radius: 2px; background: rgba(255, 255, 255, 0.1); }
progressbar.progress progress { min-height: 3px; border-radius: 2px; background: #ffffff; }
calendar { background: rgba(255, 255, 255, 0.06); border: 1px solid #333333; border-radius: 6px; padding: 6px; }
calendar > header { border: none; }
calendar > header > button { min-width: 28px; min-height: 28px; border-radius: 6px; }
calendar > header > button:hover { background: @hover; }
calendar > grid > label.day-name { color: #888888; font-weight: bold; font-size: 8.5pt; }
calendar > grid > label.day-number { padding: 6px; border-radius: 6px; }
calendar > grid > label.day-number.other-month { color: #888888; }
calendar > grid > label.day-number:selected { background: #ffffff; color: #000000; font-weight: bold; }
scrolledwindow { background: none; }
* { font-family: "Iosevka"; font-size: 11pt; color: #ffffff; }
.dot { min-width: 8px; min-height: 8px; border-radius: 4px; background: #666666; transition: background 100ms; }
.dot.focused { background: #ffffff; }
.dot:hover { background: #ffffff; }
text.query { background: none; border: none; box-shadow: none; padding: 0; }
.hit { padding: 0 10px; }
.hit:hover { background: @hover; }
.hit.picked { background: #ffffff; color: #000000; }
window.lock { background: #000000; }
.lock-time { font-size: 64pt; }
passwordentry.lock-entry { background: #000000; border: 1px solid #ffffff; border-radius: 6px; min-width: 320px; min-height: 40px; }
passwordentry.lock-entry:disabled { border-color: #888888; }
/* a block of the bar: as a tab (.tab) it changes colours alone, the same border (transparent here) and margins
   either way, so nothing in the bar moves as a panel opens */
.pill { padding: 0 8px; margin: 2px 0 0 0; border: 1px solid transparent; border-bottom-width: 0;
        border-radius: 6px 6px 0 0; transition: background 100ms; }
/* the bar's black laid by its parts, not under the whole window: a block's slot is black but for the block
   hovered or a tab, then its ground is the panel's straight over the wallpaper (the same grey, the same blur) and
   the black left round its corners its shadow, clipped to the slot */
window.bar { background: transparent; }
.bar-bg, .slot { background: rgba(0, 0, 0, ALPHA); }
.slot:hover, .slot.tab { background: transparent; }
.slot:hover > .pill, .slot.tab > .pill { background: @panel; box-shadow: 0 0 0 30px rgba(0, 0, 0, ALPHA); }
.tray-item { padding: 0 5px; }
image { -gtk-icon-size: 16px; }
/* the tray's menus in the panels' look, no outline */
popover > contents { background: @panel; border: none; border-radius: 10px; padding: 6px; box-shadow: none; }
popover button { background: none; border: none; box-shadow: none; padding: 6px 12px; border-radius: 6px; }
popover button:hover { background: @hover; }
popover separator { background: rgba(255, 255, 255, 0.1); margin: 4px 6px; }
"#;

/// The bar's mode, i3's bar mode toggle: docked (its strip taken from the windows), or hidden and shown over
/// the windows while Super is held (ostrov bar toggle, peek, unpeek: config/hypr's binds).
struct Mode {
    win: gtk4::ApplicationWindow,
    docked: std::cell::Cell<bool>,
    peeking: std::cell::Cell<bool>,
    launching: std::cell::Cell<bool>,
}

impl Mode {
    fn apply(&self) {
        if self.docked.get() {
            self.win.set_layer(Layer::Top);
            self.win.auto_exclusive_zone_enable();
        } else {
            self.win.set_layer(Layer::Overlay);
            self.win.set_exclusive_zone(0);
        }
        // the launcher in it takes the keyboard while open, and shows it hidden or not
        self.win.set_keyboard_mode(if self.launching.get() { KeyboardMode::Exclusive } else { KeyboardMode::None });
        self.win.set_visible(self.docked.get() || self.peeking.get() || self.launching.get());
    }
}

thread_local! {
    /// What `ostrov ARGS` does in the running ostrov, set once it is built.
    static COMMAND: std::cell::RefCell<Option<Box<dyn Fn(&[String]) -> Result<(), String>>>> = Default::default();
}

const USAGE: &str = "usage: ostrov [panel | menu NAME | calendar | run | clip | lock | bar toggle|peek|unpeek]";

/// What the tray shows of an item: its address, its icon, its menu.
#[derive(Clone)]
struct TrayEntry {
    address: String,
    item: StatusNotifierItem,
    menu: Option<TrayMenu>,
}

/// The tray in its own Tokio runtime: every change sends the whole list over; activations come back.
fn tray(tx: async_channel::Sender<Vec<TrayEntry>>, mut rx: tokio::sync::mpsc::UnboundedReceiver<ActivateRequest>) {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    rt.block_on(async move {
        let Ok(client) = Client::new().await else { return };
        let client = std::sync::Arc::new(client);
        let mut events = client.subscribe();
        let snapshot = |client: &Client| -> Vec<TrayEntry> {
            let map = client.items();
            let map = map.lock().unwrap();
            let mut v: Vec<TrayEntry> = map
                .iter()
                .map(|(a, (item, menu))| TrayEntry { address: a.clone(), item: item.clone(), menu: menu.clone() })
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
fn tray_icon(item: &StatusNotifierItem) -> gtk4::Image {
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

/// A menu's entries into a box of buttons, a submenu's entries indented under its own.
fn fill_menu(
    bx: &gtk4::Box,
    items: &[MenuItem],
    depth: i32,
    at: (&str, &str),
    act: &tokio::sync::mpsc::UnboundedSender<ActivateRequest>,
    pop: &gtk4::Popover,
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
        b.set_sensitive(it.enabled);
        b.set_margin_start(depth * 12);
        if let Some(l) = b.child().and_downcast::<gtk4::Label>() {
            l.set_xalign(0.0);
        }
        if it.submenu.is_empty() {
            let (act, address, path, id, pop) = (act.clone(), at.0.to_string(), at.1.to_string(), it.id, pop.clone());
            b.connect_clicked(move |_| {
                let _ = act.send(ActivateRequest::MenuItem { address: address.clone(), menu_path: path.clone(), submenu_id: id });
                pop.popdown();
            });
            bx.append(&b);
        } else {
            b.set_sensitive(false);
            bx.append(&b);
            fill_menu(bx, &it.submenu, depth + 1, at, act, pop);
        }
    }
}

fn volume_icon((v, muted): (f64, bool)) -> &'static str {
    if muted || v == 0.0 {
        "audio-volume-muted-symbolic"
    } else if v < 0.34 {
        "audio-volume-low-symbolic"
    } else if v < 0.67 {
        "audio-volume-medium-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }
}

fn pill() -> gtk4::Box {
    let b = gtk4::Box::new(gtk4::Orientation::Horizontal, 7);
    b.add_css_class("pill");
    b
}

/// A block's place in the bar: the black around it, gone while it is hovered or a tab (see the CSS).
fn slot(w: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let s = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    s.add_css_class("slot");
    s.set_overflow(gtk4::Overflow::Hidden);
    s.append(w);
    s
}

fn activate(app: &gtk4::Application) {
    let win = gtk4::ApplicationWindow::new(app);
    win.init_layer_shell();
    win.set_layer(Layer::Top);
    win.set_namespace(Some("ostrov"));
    win.add_css_class("bar");
    for e in [Edge::Top, Edge::Left, Edge::Right] {
        win.set_anchor(e, true);
    }
    win.auto_exclusive_zone_enable();
    win.set_default_size(-1, 25);

    let display = gdk::Display::default().unwrap();
    // the bar's see-through, bin/theme's alpha (1 under dark), followed as bin/theme rewrites it
    let css = gtk4::CssProvider::new();
    let alpha_file = home().join(".cache/theme/alpha");
    let load = {
        let (css, f) = (css.clone(), alpha_file.clone());
        move || {
            let alpha = std::fs::read_to_string(&f).unwrap_or("1".into());
            css.load_from_string(&CSS.replace("ALPHA", alpha.trim()));
        }
    };
    load();
    gtk4::style_context_add_provider_for_display(&display, &css, 900);
    if let Ok(mon) = gtk4::gio::File::for_path(&alpha_file).monitor_file(gtk4::gio::FileMonitorFlags::NONE, gtk4::gio::Cancellable::NONE) {
        mon.connect_changed(move |_, _, _, _| load());
        // kept for the program's life
        std::mem::forget(mon);
    }
    gtk4::IconTheme::for_display(&display).set_theme_name(Some("Adwaita"));

    let bar = gtk4::CenterBox::new();

    // the workspaces
    let dots = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    dots.set_valign(gtk4::Align::Center);
    // the prototype's mark, so it is not taken for the Quickshell bar
    let start = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    start.add_css_class("bar-bg");
    start.set_hexpand(true);
    dots.set_margin_start(12);
    start.append(&dots);
    bar.set_start_widget(Some(&start));
    // a dot a workspace, made once and kept, so the focused one's class alone changes and CSS animates it
    // the focused dot's growing and the last one's shrinking, Quickshell's way: every frame a step of each
    // dot's width towards its own (28 focused, 8 not), about 100 ms in all, the frame clock let go once there
    let growing = std::rc::Rc::new(std::cell::Cell::new(false));
    let grow = {
        let (dots, growing) = (dots.clone(), growing.clone());
        move || {
            if growing.replace(true) {
                return;
            }
            let last = std::cell::Cell::new(0i64);
            let growing = growing.clone();
            dots.add_tick_callback(move |dots, clock| {
                let now = clock.frame_time();
                let dt = if last.get() == 0 { 16_000 } else { now - last.get() };
                last.set(now);
                let k = 1.0 - (-(dt as f64) / 30_000.0).exp();
                let mut moving = false;
                let mut c = dots.first_child();
                while let Some(dot) = c {
                    let to = if dot.has_css_class("focused") { 28.0 } else { 8.0 };
                    let w = dot.width_request().max(8) as f64;
                    let step = (to - w) * k;
                    let next = if (to - w).abs() < 1.0 { to } else { w + if step.abs() < 1.0 { step.signum() } else { step } };
                    dot.set_size_request(next.round() as i32, 8);
                    moving |= next != to;
                    c = dot.next_sibling();
                }
                if moving {
                    glib::ControlFlow::Continue
                } else {
                    growing.set(false);
                    glib::ControlFlow::Break
                }
            });
        }
    };
    let draw_dots = {
        let dots = dots.clone();
        let made: std::rc::Rc<std::cell::RefCell<Vec<(i64, gtk4::Box)>>> = Default::default();
        move || {
            let (ids, active) = wm::workspaces();
            let mut made = made.borrow_mut();
            // gone workspaces out, new ones in at their place
            made.retain(|(id, dot)| {
                let keep = ids.contains(id);
                if !keep {
                    dots.remove(dot);
                }
                keep
            });
            for (i, id) in ids.iter().enumerate() {
                if made.iter().any(|(m, _)| m == id) {
                    continue;
                }
                let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                dot.add_css_class("dot");
                dot.set_valign(gtk4::Align::Center);
                let id = *id;
                let click = gtk4::GestureClick::new();
                click.connect_released(move |_, _, _, _| {
                    wm::go(id);
                });
                dot.add_controller(click);
                dot.set_cursor_from_name(Some("pointer"));
                let after = if i == 0 { None } else { made.iter().find(|(m, _)| *m == ids[i - 1]).map(|(_, d)| d.clone()) };
                dots.insert_child_after(&dot, after.as_ref());
                let at = i.min(made.len());
                made.insert(at, (id, dot));
            }
            for (id, dot) in made.iter() {
                if Some(*id) == active {
                    dot.add_css_class("focused");
                } else {
                    dot.remove_css_class("focused");
                }
            }
            grow();
        }
    };
    draw_dots();
    let mode = std::rc::Rc::new(Mode { win: win.clone(), docked: true.into(), peeking: false.into(), launching: false.into() });
    let (ws_tx, ws_rx) = async_channel::unbounded();
    std::thread::spawn(move || wm::events(ws_tx));
    let m = mode.clone();
    glib::spawn_future_local(async move {
        while let Ok(e) = ws_rx.recv().await {
            match e {
                wm::Event::Workspaces => draw_dots(),
                wm::Event::Done if m.peeking.get() => {
                    m.peeking.set(false);
                    m.apply();
                }
                wm::Event::Done => {}
            }
        }
    });

    // the clock and the weather
    let mid = pill();
    let weather_icon = gtk4::Image::new();
    let weather = gtk4::Label::new(None);
    let clock = gtk4::Label::new(None);
    mid.append(&weather_icon);
    mid.append(&weather);
    mid.append(&clock);
    let mid_slot = slot(&mid);
    bar.set_center_widget(Some(&mid_slot));

    // the right: layout, tray, status
    let right = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    right.set_hexpand(true);
    let fill = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    fill.add_css_class("bar-bg");
    fill.set_hexpand(true);
    let layout = gtk4::Label::new(Some("US"));
    layout.add_css_class("pill");
    let tray_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let status = pill();
    let wifi = gtk4::Image::from_icon_name("network-wireless-offline-symbolic");
    let vol = gtk4::Image::from_icon_name("audio-volume-high-symbolic");
    let bat = gtk4::Image::from_icon_name("battery-missing-symbolic");
    status.append(&wifi);
    status.append(&vol);
    status.append(&bat);
    let status_slot = slot(&status);
    let end = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    end.add_css_class("bar-bg");
    end.set_size_request(6, -1);
    right.append(&fill);
    let layout_slot = slot(&layout);
    right.append(&layout_slot);
    right.append(&tray_box);
    right.append(&status_slot);
    right.append(&end);
    bar.set_end_widget(Some(&right));

    // once a second: the clock
    let tick = move || {
        if let Ok(now) = glib::DateTime::now_local() {
            clock.set_text(&now.format("%a %b %-d  %H:%M").unwrap_or_default());
        }
        glib::ControlFlow::Continue
    };
    let _ = tick.clone()();
    glib::timeout_add_seconds_local(1, tick);

    // wmd's state: Wi-Fi, the weather, the layout, the battery
    let hub = Hub::start();
    let panel = panel::build(app, &hub, &status_slot);
    let notes = notes::start(app);
    let cal = calendar::build(app, &hub, &mid_slot, &notes);
    // a click on the clock opens the calendar, one popup open at a time
    {
        let (cal, pp) = (cal.clone(), panel.popup.clone());
        let click = gtk4::GestureClick::new();
        click.connect_released(move |_, _, _, _| {
            pp.close();
            cal.toggle();
        });
        mid.add_controller(click);
        mid.set_cursor_from_name(Some("pointer"));
    }
    hub.on(move |s| {
        let w = &s["wifi"];
        let icon = if !w["on"].as_bool().unwrap_or(false) {
            "network-wireless-disabled-symbolic".to_string()
        } else if w["ssid"].as_str().unwrap_or("").is_empty() {
            "network-wireless-offline-symbolic".to_string()
        } else {
            let bars = ["none", "weak", "ok", "good", "excellent"];
            format!("network-wireless-signal-{}-symbolic", bars[w["signal"].as_u64().unwrap_or(0).min(4) as usize])
        };
        wifi.set_icon_name(Some(&icon));
        layout.set_text(if s["keymap"].as_str().unwrap_or("").contains("Russian") { "RU" } else { "US" });
        let wt = s["weather"].as_object();
        weather_icon.set_visible(wt.is_some());
        weather.set_visible(wt.is_some());
        if let Some(wt) = wt {
            weather_icon.set_icon_name(wt["icon"].as_str());
            weather.set_text(&format!("{}°", wt["temp"]));
        }
        let a = &s["audio"];
        vol.set_icon_name(Some(volume_icon((a["volume"].as_f64().unwrap_or(0.0), a["muted"].as_bool().unwrap_or(false)))));
        let b = &s["battery"];
        bat.set_visible(b["present"].as_bool().unwrap_or(false));
        bat.set_icon_name(b["icon"].as_str());
        bat.set_tooltip_text(Some(&format!("{}%{}", b["percent"].as_f64().unwrap_or(0.0).round(), {
            let t = panel::battery_time(b);
            if t.is_empty() { t } else { format!(", {t}") }
        })));
    });
    // the launcher, in the clock's place
    let launcher_widget: std::rc::Rc<std::cell::OnceCell<gtk4::Box>> = Default::default();
    let launcher = {
        let lw = launcher_widget.clone();
        let (m, mid, mid_slot, bar, dots, layout_slot) =
            (mode.clone(), mid.clone(), mid_slot.clone(), bar.clone(), dots.clone(), layout_slot.clone());
        launcher::Launcher::new(move |open| {
            // over the bar from the workspaces to the right's blocks, the clock out of the way under it
            if open {
                let x = |w: &gtk4::Box| w.compute_bounds(&bar).map_or(0, |b| b.x() as i32);
                let start = x(&dots) + dots.width();
                if let Some(w) = lw.get() {
                    w.set_margin_start(start + 12);
                    w.set_margin_end(bar.width() - x(&layout_slot) + 12);
                }
            }
            // the clock's slot keeps its black, the clock in it hidden
            mid.set_opacity(if open { 0.0 } else { 1.0 });
            mid_slot.set_can_target(!open);
            m.launching.set(open);
            m.apply();
        })
    };
    let _ = launcher_widget.set(launcher.widget.clone());
    launcher.widget.set_margin_start(0);
    let over = gtk4::Overlay::new();
    over.add_overlay(&launcher.widget);
    let lock = lock::build(app);

    // ostrov ARGS, from a key or a script, handed over to this ostrov by GApplication
    {
        let (panel, cal) = (panel.clone(), cal.clone());
        COMMAND.with(|c| {
            *c.borrow_mut() = Some(Box::new(move |args: &[String]| {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                match args[..] {
                    ["panel"] => {
                        cal.close();
                        panel.toggle();
                    }
                    ["menu", name] => {
                        cal.close();
                        panel.open_menu(name);
                    }
                    ["run"] => launcher.toggle(false),
                    ["clip"] => launcher.toggle(true),
                    ["lock"] => lock.lock(),
                    ["calendar"] => {
                        panel.popup.close();
                        cal.toggle();
                    }
                    ["bar", what @ ("toggle" | "peek" | "unpeek")] => {
                        match what {
                            "toggle" => mode.docked.set(!mode.docked.get()),
                            "peek" => mode.peeking.set(true),
                            _ => mode.peeking.set(false),
                        }
                        mode.apply();
                    }
                    _ => return Err(USAGE.into()),
                }
                Ok(())
            }))
        });
    }
    // a click elsewhere in the bar closes them (the bar takes no keyboard, so they keep theirs)
    {
        let (panel, cal, mid_slot, status_slot) = (panel.clone(), cal.clone(), mid_slot.clone(), status_slot.clone());
        let click = gtk4::GestureClick::new();
        click.connect_released(move |g, _, x, y| {
            let Some(w) = g.widget() else { return };
            let hit = w.pick(x, y, gtk4::PickFlags::DEFAULT);
            if !hit.is_some_and(|h| h.is_ancestor(&mid_slot) || h.is_ancestor(&status_slot)) {
                panel.popup.close();
                cal.close();
            }
        });
        win.add_controller(click);
    }
    // a click on the status opens the quick settings
    let click = gtk4::GestureClick::new();
    click.connect_released(move |_, _, _, _| {
        cal.close();
        panel.toggle();
    });
    status.add_controller(click);
    status.set_cursor_from_name(Some("pointer"));

    // the tray: a click activates, a right click opens its menu in a popover
    let (tray_tx, tray_rx) = async_channel::unbounded::<Vec<TrayEntry>>();
    let (act_tx, act_rx) = tokio::sync::mpsc::unbounded_channel::<ActivateRequest>();
    std::thread::spawn(move || tray(tray_tx, act_rx));
    glib::spawn_future_local(async move {
        while let Ok(items) = tray_rx.recv().await {
            while let Some(c) = tray_box.first_child() {
                tray_box.remove(&c);
            }
            for e in items {
                let cell = pill();
                cell.add_css_class("tray-item");
                cell.append(&tray_icon(&e.item));
                cell.set_cursor_from_name(Some("pointer"));
                let click = gtk4::GestureClick::new();
                click.set_button(0);
                let (act, cell2) = (act_tx.clone(), cell.clone());
                click.connect_released(move |g, _, _, _| {
                    if g.current_button() == 3 || e.item.item_is_menu {
                        let Some(menu) = &e.menu else { return };
                        let pop = gtk4::Popover::new();
                        pop.set_has_arrow(false);
                        pop.set_parent(&cell2);
                        let bx = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                        let path = e.item.menu.clone().unwrap_or_default();
                        fill_menu(&bx, &menu.submenus, 0, (&e.address, &path), &act, &pop);
                        pop.set_child(Some(&bx));
                        pop.connect_closed(|p| p.unparent());
                        pop.popup();
                    } else {
                        let _ = act.send(ActivateRequest::Default { address: e.address.clone(), x: 0, y: 0 });
                    }
                });
                cell.add_controller(click);
                tray_box.append(&slot(&cell));
            }
        }
    });

    over.set_child(Some(&bar));
    win.set_child(Some(&over));
    win.present();
}

fn main() -> glib::ExitCode {
    // one ostrov: run again, it hands its arguments to the running one and exits
    let app = gtk4::Application::builder()
        .application_id("dev.danil.ostrov")
        .flags(gtk4::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    app.connect_activate(activate);
    app.connect_command_line(|app, cl| {
        if COMMAND.with(|c| c.borrow().is_none()) {
            app.activate();
        }
        let args: Vec<String> = cl.arguments().iter().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
        if args.is_empty() {
            return glib::ExitCode::SUCCESS;
        }
        match COMMAND.with(|c| c.borrow().as_ref().map(|f| f(&args))) {
            Some(Err(e)) => {
                cl.printerr_literal(&format!("{e}\n"));
                glib::ExitCode::FAILURE
            }
            _ => glib::ExitCode::SUCCESS,
        }
    });
    app.run()
}
