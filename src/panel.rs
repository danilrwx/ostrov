//! The quick settings, as the Quickshell bar's (config/quickshell/QuickSettings.qml): a popover under the bar's
//! status. Up top the battery and round buttons for a screenshot, the lock and the power menu; then the
//! sliders, volume and mic (their devices behind an arrow) and brightness (night light behind its arrow); then
//! the toggles two a row, each a split pill: the left side flips it, the arrow opens its menu under its row,
//! one menu at a time, sliding open in 100 ms. Wi-Fi with its networks (a new one asks its passphrase right
//! under it, a right click forgets a known one), Bluetooth (paired devices, a scan, pairing), the power mode,
//! the wallpaper, OpenVPN, VLESS, the headset's mode. Closed by a click outside it or Escape. Everything from
//! wmd's state; the switching through wmd and the dotfiles' scripts.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use serde_json::Value;

use crate::style::{clear, label};
use crate::hub::{bin, run, run_then, s, wmd, Hub};
use crate::popup::{Popup, Side};

fn wmdc(args: &[&str]) {
    let w = wmd().to_string_lossy().into_owned();
    let mut v = vec![w.as_str()];
    v.extend_from_slice(args);
    run(&v);
}



/// A menu row: an icon, its text, a note, a tick while on (the row inverted); a click picks it.
fn row(icon: &str, text: &str, note: &str, on: bool, pick: impl Fn() + 'static) -> gtk4::Button {
    let b = gtk4::Button::new();
    b.add_css_class("item");
    if on {
        b.add_css_class("on");
    }
    let bx = gtk4::Box::new(Orientation::Horizontal, 10);
    if !icon.is_empty() {
        bx.append(&gtk4::Image::from_icon_name(icon));
    }
    let t = label(text, "");
    t.set_hexpand(true);
    t.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    bx.append(&t);
    if !note.is_empty() {
        bx.append(&label(note, "dim"));
    }
    if on {
        bx.append(&gtk4::Image::from_icon_name("object-select-symbolic"));
    }
    b.set_child(Some(&bx));
    b.connect_clicked(move |_| pick());
    b
}

/// A right click on a widget runs f.
fn on_right_click(w: &impl IsA<gtk4::Widget>, f: impl Fn() + 'static) {
    let g = gtk4::GestureClick::new();
    g.set_button(3);
    g.connect_released(move |_, _, _, _| f());
    w.add_controller(g);
}

/// The menus of the panel by name, one unfolded at a time, and their arrows to turn.
#[derive(Default)]
struct Menus {
    open: RefCell<String>,
    revealers: RefCell<HashMap<String, gtk4::Revealer>>,
    arrows: RefCell<HashMap<String, Vec<gtk4::Widget>>>,
}

impl Menus {
    fn flip(&self, name: &str) {
        let now = if *self.open.borrow() == name { String::new() } else { name.to_string() };
        self.set(&now);
    }
    fn set(&self, name: &str) {
        *self.open.borrow_mut() = name.to_string();
        for (n, r) in self.revealers.borrow().iter() {
            // shown before it slides open, hidden once it has slid shut: a folded menu takes no spacing
            if n == name {
                r.set_visible(true);
            }
            r.set_reveal_child(n == name);
        }
        for (n, ws) in self.arrows.borrow().iter() {
            for w in ws {
                if n == name {
                    w.add_css_class("open");
                } else {
                    w.remove_css_class("open");
                }
            }
        }
    }
    /// A menu: a header (its icon on a white square, its title) over a box its items go into.
    fn menu(&self, name: &str, icon: &str, title: &str) -> (gtk4::Revealer, gtk4::Box) {
        let r = gtk4::Revealer::new();
        r.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        r.set_transition_duration(100);
        let card = gtk4::Box::new(Orientation::Vertical, 2);
        card.add_css_class("menu");
        let head = gtk4::Box::new(Orientation::Horizontal, 10);
        head.add_css_class("menu-head");
        let badge = gtk4::Image::from_icon_name(icon);
        badge.add_css_class("badge");
        head.append(&badge);
        head.append(&label(title, "title"));
        card.append(&head);
        let items = gtk4::Box::new(Orientation::Vertical, 0);
        card.append(&items);
        r.set_child(Some(&card));
        r.set_visible(false);
        r.connect_child_revealed_notify(|r| {
            if !r.is_child_revealed() && !r.reveals_child() {
                r.set_visible(false);
            }
        });
        self.revealers.borrow_mut().insert(name.to_string(), r.clone());
        (r, items)
    }
    fn arrow(self: &Rc<Self>, name: &str) -> gtk4::Button {
        let b = gtk4::Button::from_icon_name("go-next-symbolic");
        b.add_css_class("arrow");
        let (m, n) = (self.clone(), name.to_string());
        b.connect_clicked(move |_| m.flip(&n));
        self.arrows.borrow_mut().entry(name.to_string()).or_default().push(b.clone().upcast());
        b
    }
}

/// A split toggle: the pill flips it (inverted while on), the arrow side opens its menu.
struct Toggle {
    root: gtk4::Box,
    icon: gtk4::Image,
    sub: gtk4::Label,
}

impl Toggle {
    fn new(menus: &Rc<Menus>, icon: &str, title: &str, menu: Option<&str>, flip: impl Fn() + 'static) -> Toggle {
        let root = gtk4::Box::new(Orientation::Horizontal, 0);
        root.add_css_class("toggle");
        root.set_hexpand(true);
        root.set_homogeneous(false);
        let main = gtk4::Button::new();
        main.add_css_class("toggle-main");
        main.set_hexpand(true);
        let bx = gtk4::Box::new(Orientation::Horizontal, 10);
        let img = gtk4::Image::from_icon_name(icon);
        bx.append(&img);
        let col = gtk4::Box::new(Orientation::Vertical, 0);
        col.set_valign(Align::Center);
        let t = label(title, "toggle-title");
        let sub = label("", "toggle-sub");
        sub.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        sub.set_max_width_chars(14);
        col.append(&t);
        col.append(&sub);
        bx.append(&col);
        main.set_child(Some(&bx));
        main.connect_clicked(move |_| flip());
        root.append(&main);
        if let Some(m) = menu {
            let a = menus.arrow(m);
            a.add_css_class("toggle-side");
            root.append(&a);
        }
        Toggle { root, icon: img, sub }
    }
    fn set(&self, on: bool, icon: &str, sub: &str) {
        if on {
            self.root.add_css_class("on");
        } else {
            self.root.remove_css_class("on");
        }
        if !icon.is_empty() {
            self.icon.set_icon_name(Some(icon));
        }
        self.sub.set_text(sub);
        self.sub.set_visible(!sub.is_empty());
    }
}

/// A slider: an icon button (mute), a scale that runs moved with its value as the user moves it, an arrow.
/// The state's value comes back in through set, but not while the user is at it.
struct Slider {
    root: gtk4::Box,
    icon: gtk4::Button,
    scale: gtk4::Scale,
    touched: Rc<RefCell<Instant>>,
}

impl Slider {
    fn new(icon: &str, moved: impl Fn(f64) + 'static) -> Slider {
        let root = gtk4::Box::new(Orientation::Horizontal, 6);
        root.add_css_class("slider");
        let ib = gtk4::Button::from_icon_name(icon);
        ib.add_css_class("flat-round");
        let scale = gtk4::Scale::with_range(Orientation::Horizontal, 0.0, 1.0, 0.01);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        let touched = Rc::new(RefCell::new(Instant::now() - Duration::from_secs(10)));
        let t = touched.clone();
        scale.connect_change_value(move |_, _, v| {
            *t.borrow_mut() = Instant::now();
            moved(v.clamp(0.0, 1.0));
            glib::Propagation::Proceed
        });
        root.append(&ib);
        root.append(&scale);
        Slider { root, icon: ib, scale, touched }
    }
    fn set(&self, v: f64, icon: &str) {
        if self.touched.borrow().elapsed() > Duration::from_millis(800) {
            self.scale.set_value(v);
        }
        self.icon.set_icon_name(icon);
    }
}

fn level_icon(kind: &str, v: f64, muted: bool) -> String {
    let lv = if muted || v == 0.0 {
        "muted"
    } else if v < 0.34 {
        "low"
    } else if v < 0.67 {
        "medium"
    } else {
        "high"
    };
    format!("{kind}-{lv}-symbolic")
}

fn duration(secs: i64) -> String {
    let (h, m) = (secs / 3600, (secs % 3600 + 30) / 60);
    if h > 0 { format!("{h} h {m:02} min") } else { format!("{m} min") }
}

pub fn battery_time(b: &Value) -> String {
    match s(b, &["state"]) {
        "charging" => match b["toFull"].as_i64().unwrap_or(0) {
            0 => "charging".into(),
            t => format!("full in {}", duration(t)),
        },
        "fully-charged" => "full".into(),
        "pending-charge" => "not charging".into(),
        _ => match b["toEmpty"].as_i64().unwrap_or(0) {
            0 => String::new(),
            t => format!("{} left", duration(t)),
        },
    }
}

pub struct Panel {
    pub popup: Rc<Popup>,
    menus: Rc<Menus>,
}

impl Panel {
    pub fn toggle(&self) {
        self.popup.toggle()
    }

    /// Open with one menu unfolded (ostrov menu wifi: a key to it, or a look at it without a click).
    pub fn open_menu(&self, name: &str) {
        self.popup.open();
        self.menus.set(name);
    }
}

pub fn build(host: &Rc<crate::popup::Host>, hub: &Rc<Hub>, tab: &gtk4::Box) -> Rc<Panel> {
    let menus = Rc::new(Menus::default());
    // closes the panel, once the popup is made (the buttons that close it are made before it)
    let closer: Rc<RefCell<Option<Rc<Popup>>>> = Rc::default();
    let panel = {
        let c = closer.clone();
        Rc::new(move || {
            if let Some(p) = c.borrow().as_ref() {
                p.close()
            }
        })
    };

    let col = gtk4::Box::new(Orientation::Vertical, 10);
    col.add_css_class("surface");

    // the header: the battery, then a screenshot, the lock, the power menu
    let head = gtk4::Box::new(Orientation::Horizontal, 8);
    let bat = gtk4::Box::new(Orientation::Horizontal, 8);
    bat.add_css_class("battery");
    let bat_icon = gtk4::Image::from_icon_name("battery-missing-symbolic");
    let bat_pct = label("", "bold");
    let bat_time = label("", "dim");
    bat_time.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    bat.append(&bat_icon);
    bat.append(&bat_pct);
    bat.append(&bat_time);
    head.append(&bat);
    let spacer = gtk4::Box::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    head.append(&spacer);
    let round = |icon: &str, f: Box<dyn Fn()>| {
        let b = gtk4::Button::from_icon_name(icon);
        b.add_css_class("round");
        b.connect_clicked(move |_| f());
        b
    };
    let p = panel.clone();
    head.append(&round("applets-screenshooter-symbolic", Box::new(move || {
        p();
        run(&[&bin("screenshot-select")]);
    })));
    let p = panel.clone();
    head.append(&round("system-lock-screen-symbolic", Box::new(move || {
        p();
        run(&["loginctl", "lock-session"]);
    })));
    let power = menus.arrow("system");
    power.set_icon_name("system-shutdown-symbolic");
    power.remove_css_class("arrow");
    power.add_css_class("round");
    head.append(&power);
    col.append(&head);

    let (sys, sys_items) = menus.menu("system", "system-shutdown-symbolic", "Power Off");
    for (icon, text, cmd) in [
        ("weather-clear-night-symbolic", "Suspend", vec!["systemctl", "suspend"]),
        ("view-refresh-symbolic", "Restart…", vec!["systemctl", "reboot"]),
        ("system-shutdown-symbolic", "Power Off…", vec!["systemctl", "poweroff"]),
        ("system-log-out-symbolic", "Log Out", vec!["sh", "-c", "hyprctl dispatch exit || swaymsg exit"]),
    ] {
        let p = panel.clone();
        sys_items.append(&row(icon, text, "", false, move || {
            p();
            run(&cmd);
        }));
    }
    col.append(&sys);

    // the sliders
    let vol = Slider::new("audio-volume-high-symbolic", |v| run(&["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{v:.2}")]));
    vol.icon.connect_clicked(|_| run(&["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]));
    vol.root.append(&menus.arrow("outs"));
    col.append(&vol.root);
    let (outs, outs_items) = menus.menu("outs", "audio-speakers-symbolic", "Sound Output");
    col.append(&outs);
    let mic = Slider::new("microphone-sensitivity-high-symbolic", |v| run(&["wpctl", "set-volume", "@DEFAULT_AUDIO_SOURCE@", &format!("{v:.2}")]));
    mic.icon.connect_clicked(|_| run(&["wpctl", "set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"]));
    mic.root.append(&menus.arrow("ins"));
    col.append(&mic.root);
    let (ins, ins_items) = menus.menu("ins", "audio-input-microphone-symbolic", "Sound Input");
    col.append(&ins);
    let bri = Slider::new("display-brightness-symbolic", |v| wmdc(&["brightness", &format!("{}", (v * 100.0).round() as i64)]));
    bri.root.append(&menus.arrow("night"));
    col.append(&bri.root);

    // night light: the mode, the hours, the warmth, shown on the screen while dragged
    let (night, night_items) = menus.menu("night", "night-light-symbolic", "Night Light");
    let night_modes = gtk4::Box::new(Orientation::Vertical, 0);
    night_items.append(&night_modes);
    let hours = gtk4::Box::new(Orientation::Horizontal, 8);
    hours.set_margin_start(36);
    let from = gtk4::Entry::new();
    let to = gtk4::Entry::new();
    for (name, e) in [("from", &from), ("to", &to)] {
        e.set_max_width_chars(5);
        e.set_width_chars(5);
        hours.append(&label(name, "dim"));
        hours.append(e);
    }
    let (f2, t2) = (from.clone(), to.clone());
    let set_hours = move || wmdc(&["night", "time", &f2.text(), &t2.text()]);
    let sh = Rc::new(set_hours);
    let s1 = sh.clone();
    from.connect_activate(move |_| s1());
    let s2 = sh.clone();
    to.connect_activate(move |_| s2());
    night_items.append(&hours);
    let warm_head = gtk4::Box::new(Orientation::Horizontal, 0);
    warm_head.set_margin_top(6);
    let wl = label("Warmth", "dim");
    wl.set_hexpand(true);
    let kelvin = label("", "dim");
    warm_head.append(&wl);
    warm_head.append(&kelvin);
    night_items.append(&warm_head);
    let warm_save: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let (k2, ws) = (kelvin.clone(), warm_save.clone());
    let warm = Slider::new("night-light-symbolic", move |v| {
        let k = (6500.0 - v * 4000.0).round() as i64;
        k2.set_text(&format!("{k} K"));
        wmdc(&["night", "preview", &k.to_string()]);
        // kept once the slider rests
        if let Some(id) = ws.borrow_mut().take() {
            id.remove();
        }
        let ws2 = ws.clone();
        *ws.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_millis(500), move || {
            ws2.borrow_mut().take();
            wmdc(&["night", "temp", &k.to_string()]);
        }));
    });
    night_items.append(&warm.root);
    col.append(&night);

    // the toggles and their menus
    let grid_row = |a: &gtk4::Box, b: Option<&gtk4::Box>| {
        let r = gtk4::Box::new(Orientation::Horizontal, 10);
        r.set_homogeneous(true);
        r.append(a);
        match b {
            Some(b) => r.append(b),
            None => r.append(&gtk4::Box::new(Orientation::Horizontal, 0)),
        }
        r
    };
    let state = Rc::new(RefCell::new(Value::Null));

    let st = state.clone();
    let wifi = Toggle::new(&menus, "network-wireless-symbolic", "Wi-Fi", Some("wifi"), move || {
        let on = st.borrow()["wifi"]["on"].as_bool().unwrap_or(false);
        wmdc(&["wifi", if on { "off" } else { "on" }]);
    });
    let st = state.clone();
    let bt = Toggle::new(&menus, "bluetooth-active-symbolic", "Bluetooth", Some("bt"), move || {
        let on = st.borrow()["bt"]["on"].as_bool().unwrap_or(false);
        wmdc(&["bt", if on { "off" } else { "on" }]);
    });
    col.append(&grid_row(&wifi.root, Some(&bt.root)));
    let (wifi_menu, wifi_items) = menus.menu("wifi", "network-wireless-symbolic", "Wi-Fi");
    col.append(&wifi_menu);
    let (bt_menu, bt_items) = menus.menu("bt", "bluetooth-active-symbolic", "Bluetooth");
    col.append(&bt_menu);

    let st = state.clone();
    let power_t = Toggle::new(&menus, "power-profile-balanced-symbolic", "Power Mode", Some("power"), move || {
        let order = ["power-saver", "balanced", "performance"];
        let cur = st.borrow()["power"]["active"].as_str().unwrap_or("balanced").to_string();
        let i = order.iter().position(|p| *p == cur).unwrap_or(1);
        wmdc(&["power", "set", order[(i + 1) % 3]]);
    });
    let st = state.clone();
    let wall = Toggle::new(&menus, "preferences-desktop-wallpaper-symbolic", "Wallpaper", Some("theme"), move || {
        let on = st.borrow()["theme"]["mode"].as_str() == Some("dark-wall");
        run(&[&bin("theme"), if on { "dark" } else { "dark-wall" }]);
    });
    col.append(&grid_row(&power_t.root, Some(&wall.root)));
    let (power_menu, power_items) = menus.menu("power", "power-profile-balanced-symbolic", "Power Mode");
    col.append(&power_menu);
    let (theme_menu, theme_items) = menus.menu("theme", "preferences-desktop-wallpaper-symbolic", "Wallpaper");
    col.append(&theme_menu);

    let st = state.clone();
    let ovpn = Toggle::new(&menus, "network-vpn-symbolic", "OpenVPN", Some("openvpn"), move || {
        let o = &st.borrow()["openvpn"];
        if s(o, &["profile"]).is_empty() {
            if let Some(first) = o["profiles"][0].as_str() {
                run(&[&bin("openvpn-ctl"), "on", first]);
            }
        } else {
            run(&[&bin("openvpn-ctl"), "off"]);
        }
    });
    let st = state.clone();
    let vless = Toggle::new(&menus, "network-vpn-symbolic", "VLESS", Some("vless"), move || {
        let on = s(&st.borrow()["vless"], &["how"]) != "off";
        if on { run(&[&bin("vless"), "off"]) } else { run(&[&bin("vless"), "on", "tun"]) }
    });
    col.append(&grid_row(&ovpn.root, Some(&vless.root)));
    let (ovpn_menu, ovpn_items) = menus.menu("openvpn", "network-vpn-symbolic", "OpenVPN");
    col.append(&ovpn_menu);
    let (vless_menu, vless_items) = menus.menu("vless", "network-vpn-symbolic", "VLESS");
    col.append(&vless_menu);

    let headset = Toggle::new(&menus, "audio-headphones-symbolic", "Headset", None, || wmdc(&["headset"]));
    let headset_row = grid_row(&headset.root, None);
    col.append(&headset_row);

    let popup = Popup::new(host, tab, Side::Right, 390, &col);
    *closer.borrow_mut() = Some(popup.clone());
    // every opening with the menus folded
    let m = menus.clone();
    popup.on_open(move || m.set(""));

    // Wi-Fi's passphrase asked for, and Bluetooth's pairing, kept across redraws
    let asking: Rc<RefCell<String>> = Rc::default();
    let wifi_error = Rc::new(RefCell::new(String::new()));
    let pairing: Rc<RefCell<String>> = Rc::default();
    let bt_error = Rc::new(RefCell::new(String::new()));
    let scanning = Rc::new(RefCell::new(false));
    // what each list was drawn from, so it is drawn again only when that changes
    let drawn: Rc<RefCell<HashMap<&'static str, String>>> = Rc::default();
    let redraw: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::default();

    let draw = {
        let (state, drawn, redraw) = (state.clone(), drawn.clone(), redraw.clone());
        move || {
            let st = state.borrow().clone();
            let changed = |key: &'static str, v: String| -> bool {
                let mut d = drawn.borrow_mut();
                if d.get(key) == Some(&v) {
                    return false;
                }
                d.insert(key, v);
                true
            };
            let again = {
                let r = redraw.clone();
                move || {
                    if let Some(f) = r.borrow().as_ref() {
                        f()
                    }
                }
            };

            // the battery
            let b = &st["battery"];
            bat.set_visible(b["present"].as_bool().unwrap_or(false));
            bat_icon.set_icon_name(Some(s(b, &["icon"])));
            bat_pct.set_text(&format!("{}%", b["percent"].as_f64().unwrap_or(0.0).round()));
            let t = battery_time(b);
            bat_time.set_text(&t);
            bat_time.set_visible(!t.is_empty());

            // the sliders
            let a = &st["audio"];
            let (v, m) = (a["volume"].as_f64().unwrap_or(0.0), a["muted"].as_bool().unwrap_or(false));
            vol.set(v, &level_icon("audio-volume", v, m));
            let (v, m) = (a["mic"].as_f64().unwrap_or(0.0), a["micMuted"].as_bool().unwrap_or(false));
            mic.set(v, &level_icon("microphone-sensitivity", v, m));
            bri.set(st["brightness"].as_f64().unwrap_or(0.0) / 100.0, "display-brightness-symbolic");
            for (key, list, items) in [("outs", "sinks", &outs_items), ("ins", "sources", &ins_items)] {
                if changed(key, a[list].to_string()) {
                    clear(items);
                    for d in a[list].as_array().into_iter().flatten() {
                        let id = d["id"].to_string();
                        items.append(&row("", s(d, &["name"]), "", d["def"].as_bool().unwrap_or(false), move || {
                            run(&["wpctl", "set-default", &id])
                        }));
                    }
                }
            }

            // night light
            let n = &st["night"];
            if changed("night", n.to_string()) {
                clear(&night_modes);
                let mode = s(n, &["mode"]);
                let sun = if n["located"].as_bool().unwrap_or(false) {
                    format!("{} – {}", s(n, &["sunset"]), s(n, &["sunrise"]))
                } else {
                    "no location".into()
                };
                for (m, text, note) in [
                    ("off", "Off", String::new()),
                    ("on", "Always on", String::new()),
                    ("time", "Scheduled", format!("{} – {}", s(n, &["from"]), s(n, &["to"]))),
                    ("sun", "Sunset to sunrise", sun),
                ] {
                    night_modes.append(&row("", text, &note, mode == m, move || wmdc(&["night", "mode", m])));
                }
                hours.set_visible(mode == "time");
                from.set_text(s(n, &["from"]));
                to.set_text(s(n, &["to"]));
                let k = n["temp"].as_f64().unwrap_or(4000.0);
                warm.set((6500.0 - k) / 4000.0, "night-light-symbolic");
                kelvin.set_text(&format!("{k} K"));
            }

            // Wi-Fi
            let w = &st["wifi"];
            let won = w["on"].as_bool().unwrap_or(false);
            let bars = ["none", "weak", "ok", "good", "excellent"];
            let wicon = if !won {
                "network-wireless-disabled-symbolic".to_string()
            } else if s(w, &["ssid"]).is_empty() {
                "network-wireless-offline-symbolic".to_string()
            } else {
                format!("network-wireless-signal-{}-symbolic", bars[w["signal"].as_u64().unwrap_or(0).min(4) as usize])
            };
            wifi.set(won, &wicon, if won { s(w, &["ssid"]) } else { "" });
            // the network asked for joined (wmd's state says so before the connect returns, iwd joining it as
            // soon as it has the passphrase): the passphrase is done with
            if !asking.borrow().is_empty()
                && w["networks"].as_array().into_iter().flatten().any(|n| {
                    n["connected"].as_bool().unwrap_or(false) && s(n, &["ssid"]) == asking.borrow().as_str()
                })
            {
                asking.borrow_mut().clear();
                wifi_error.borrow_mut().clear();
            }
            // while a passphrase is typed the list holds still, a redraw would drop what is typed
            let wkey = if asking.borrow().is_empty() {
                w["networks"].to_string()
            } else {
                format!("{}{}", asking.borrow(), wifi_error.borrow())
            };
            if changed("wifi", wkey) {
                clear(&wifi_items);
                for net in w["networks"].as_array().into_iter().flatten() {
                    let ssid = s(net, &["ssid"]).to_string();
                    let known = net["known"].as_bool().unwrap_or(false) || s(net, &["security"]) == "open";
                    let icon = format!("network-wireless-signal-{}-symbolic", bars[net["signal"].as_u64().unwrap_or(0).min(4) as usize]);
                    let (ask, err, again2, ssid2) = (asking.clone(), wifi_error.clone(), again.clone(), ssid.clone());
                    let r = row(&icon, &ssid, if known { "" } else { "🔒" }, net["connected"].as_bool().unwrap_or(false), move || {
                        if known {
                            wmdc(&["wifi", "connect", &ssid2]);
                        } else {
                            let now = if *ask.borrow() == ssid2 { String::new() } else { ssid2.clone() };
                            *ask.borrow_mut() = now;
                            err.borrow_mut().clear();
                            again2();
                        }
                    });
                    if net["known"].as_bool().unwrap_or(false) {
                        let ssid3 = ssid.clone();
                        on_right_click(&r, move || wmdc(&["wifi", "forget", &ssid3]));
                    }
                    wifi_items.append(&r);
                    // the passphrase right under the network asked for: Enter or Connect joins
                    if *asking.borrow() == ssid {
                        let bx = gtk4::Box::new(Orientation::Horizontal, 6);
                        bx.set_margin_start(36);
                        bx.set_margin_end(10);
                        bx.set_margin_bottom(4);
                        let pass = gtk4::PasswordEntry::new();
                        pass.set_hexpand(true);
                        let go = gtk4::Button::with_label("Connect");
                        go.add_css_class("connect");
                        bx.append(&pass);
                        bx.append(&go);
                        wifi_items.append(&bx);
                        let note = label(&wifi_error.borrow(), "error");
                        note.set_margin_start(36);
                        note.set_visible(!wifi_error.borrow().is_empty());
                        wifi_items.append(&note);
                        let join = {
                            let (pass, go, ask, err, again, ssid) =
                                (pass.clone(), go.clone(), asking.clone(), wifi_error.clone(), again.clone(), ssid.clone());
                            Rc::new(move || {
                                let p = pass.text().to_string();
                                if p.is_empty() {
                                    return;
                                }
                                go.set_label("Joining…");
                                go.set_sensitive(false);
                                let (ask, err, again) = (ask.clone(), err.clone(), again.clone());
                                let w = wmd().to_string_lossy().into_owned();
                                run_then(vec![w, "wifi".into(), "connect".into(), ssid.clone()], Some(p), move |r| {
                                    match r {
                                        Ok(()) => ask.borrow_mut().clear(),
                                        Err(e) => *err.borrow_mut() = e,
                                    }
                                    again();
                                });
                            })
                        };
                        let j = join.clone();
                        pass.connect_activate(move |_| j());
                        go.connect_clicked(move |_| join());
                        let p2 = pass.clone();
                        glib::idle_add_local_once(move || {
                            p2.grab_focus();
                        });
                    }
                }
                wifi_items.append(&gtk4::Separator::new(Orientation::Horizontal));
                wifi_items.append(&row("", "Scan", "", false, || wmdc(&["wifi", "scan"])));
            }

            // Bluetooth
            let b = &st["bt"];
            let bon = b["on"].as_bool().unwrap_or(false);
            bt.set(bon, if bon { "bluetooth-active-symbolic" } else { "bluetooth-disabled-symbolic" }, s(b, &["connected"]));
            let bkey = format!("{}{}{}{}{}", b["devices"], b["discovering"], pairing.borrow(), bt_error.borrow(), scanning.borrow());
            if changed("bt", bkey) {
                clear(&bt_items);
                let devs: Vec<&Value> = b["devices"].as_array().into_iter().flatten().collect();
                for d in devs.iter().filter(|d| d["paired"].as_bool().unwrap_or(false)) {
                    let (addr, on) = (s(d, &["address"]).to_string(), d["connected"].as_bool().unwrap_or(false));
                    let icon = format!("{}-symbolic", if s(d, &["icon"]).is_empty() { "bluetooth" } else { s(d, &["icon"]) });
                    let a2 = addr.clone();
                    let r = row(&icon, s(d, &["name"]), "", on, move || wmdc(&["bt", if on { "disconnect" } else { "connect" }, &a2]));
                    on_right_click(&r, move || wmdc(&["bt", "forget", &addr]));
                    bt_items.append(&r);
                }
                bt_items.append(&gtk4::Separator::new(Orientation::Horizontal));
                let busy = b["discovering"].as_bool().unwrap_or(false) || *scanning.borrow();
                let (sc, again2) = (scanning.clone(), again.clone());
                bt_items.append(&row("system-search-symbolic", if busy { "Scanning…" } else { "Scan for Devices" }, "", busy, move || {
                    if *sc.borrow() {
                        return;
                    }
                    *sc.borrow_mut() = true;
                    let (sc, again) = (sc.clone(), again2.clone());
                    let w = wmd().to_string_lossy().into_owned();
                    run_then(vec![w, "bt".into(), "scan".into()], None, move |_| {
                        *sc.borrow_mut() = false;
                        again();
                    });
                    again2();
                }));
                for d in devs.iter().filter(|d| !d["paired"].as_bool().unwrap_or(false)) {
                    let addr = s(d, &["address"]).to_string();
                    let icon = format!("{}-symbolic", if s(d, &["icon"]).is_empty() { "bluetooth" } else { s(d, &["icon"]) });
                    let note = if *pairing.borrow() == addr { "pairing…" } else { "pair" };
                    let (pa, err, again2) = (pairing.clone(), bt_error.clone(), again.clone());
                    bt_items.append(&row(&icon, s(d, &["name"]), note, false, move || {
                        if !pa.borrow().is_empty() {
                            return;
                        }
                        *pa.borrow_mut() = addr.clone();
                        err.borrow_mut().clear();
                        let (pa, err, again) = (pa.clone(), err.clone(), again2.clone());
                        let w = wmd().to_string_lossy().into_owned();
                        run_then(vec![w, "bt".into(), "pair".into(), addr.clone()], None, move |r| {
                            pa.borrow_mut().clear();
                            if let Err(e) = r {
                                *err.borrow_mut() = e;
                            }
                            again();
                        });
                        again2();
                    }));
                }
                if !bt_error.borrow().is_empty() {
                    bt_items.append(&label(&bt_error.borrow(), "error"));
                }
            }

            // the power mode
            let pw = s(&st["power"], &["active"]);
            let names = |p: &str| match p {
                "power-saver" => "Power Saver",
                "performance" => "Performance",
                _ => "Balanced",
            };
            power_t.set(pw != "balanced" && !pw.is_empty(), &format!("power-profile-{}-symbolic", if pw.is_empty() { "balanced" } else { pw }), names(pw));
            if changed("power", pw.to_string()) {
                clear(&power_items);
                for p in ["performance", "balanced", "power-saver"] {
                    power_items.append(&row(&format!("power-profile-{p}-symbolic"), names(p), "", pw == p, move || wmdc(&["power", "set", p])));
                }
            }

            // the wallpaper
            let th = &st["theme"];
            let won = s(th, &["mode"]) == "dark-wall";
            let wname = s(th, &["wallpaper"]).rsplit('/').next().unwrap_or("").to_string();
            wall.set(won, "", if won { &wname } else { "" });
            if changed("theme", th.to_string()) {
                clear(&theme_items);
                let walls: Vec<String> = th["wallpapers"].as_array().into_iter().flatten().filter_map(|w| w.as_str().map(String::from)).collect();
                let (cur, ws2) = (s(th, &["wallpaper"]).to_string(), walls.clone());
                theme_items.append(&row("", "Random", "", false, move || {
                    let others: Vec<&String> = ws2.iter().filter(|w| **w != cur).collect();
                    if !others.is_empty() {
                        let i = (glib::random_int() as usize) % others.len();
                        run(&[&bin("theme"), "dark-wall", others[i]]);
                    }
                }));
                theme_items.append(&gtk4::Separator::new(Orientation::Horizontal));
                for w in walls {
                    let name = w.rsplit('/').next().unwrap_or("").to_string();
                    let on = won && w == s(th, &["wallpaper"]);
                    theme_items.append(&row("", &name, "", on, move || run(&[&bin("theme"), "dark-wall", &w])));
                }
            }

            // OpenVPN
            let o = &st["openvpn"];
            let oprof = s(o, &["profile"]).to_string();
            let ready = o["ready"].as_bool().unwrap_or(false);
            ovpn.set(!oprof.is_empty(), "", &if oprof.is_empty() { String::new() } else if ready { oprof.clone() } else { format!("{oprof}, connecting") });
            if changed("openvpn", o.to_string()) {
                clear(&ovpn_items);
                for p in o["profiles"].as_array().into_iter().flatten().filter_map(|p| p.as_str()) {
                    let p = p.to_string();
                    let note = if p == oprof && !ready { "connecting" } else { "" };
                    let p2 = p.clone();
                    ovpn_items.append(&row("", &p, note, p == oprof, move || run(&[&bin("openvpn-ctl"), "on", &p2])));
                }
                ovpn_items.append(&gtk4::Separator::new(Orientation::Horizontal));
                ovpn_items.append(&row("", "Off", "", oprof.is_empty(), || run(&[&bin("openvpn-ctl"), "off"])));
            }

            // VLESS
            let v = &st["vless"];
            let how = s(v, &["how"]);
            let vsub = if how == "off" || how.is_empty() {
                String::new()
            } else if s(v, &["mode"]) == "global" {
                format!("{}, global", s(v, &["profile"]))
            } else {
                s(v, &["profile"]).to_string()
            };
            vless.set(how != "off" && !how.is_empty(), "", &vsub);
            if changed("vless", v.to_string()) {
                clear(&vless_items);
                vless_items.append(&row("", "TUN, every app", "", how == "tun", || run(&[&bin("vless"), "on", "tun"])));
                vless_items.append(&row("", "System proxy", "", how == "proxy", || run(&[&bin("vless"), "on", "proxy"])));
                if how != "off" && !how.is_empty() {
                    vless_items.append(&gtk4::Separator::new(Orientation::Horizontal));
                    let mode = s(v, &["mode"]);
                    vless_items.append(&row("", "Rule: Russia direct", "", mode == "rule", || run(&[&bin("vless"), "mode", "rule"])));
                    vless_items.append(&row("", "Global", "", mode == "global", || run(&[&bin("vless"), "mode", "global"])));
                    vless_items.append(&gtk4::Separator::new(Orientation::Horizontal));
                    let cur = s(v, &["profile"]).to_string();
                    for p in v["profiles"].as_array().into_iter().flatten().filter_map(|p| p.as_str()) {
                        let p = p.to_string();
                        let p2 = p.clone();
                        vless_items.append(&row("", &p, "", p == cur, move || run(&[&bin("vless"), "profile", &p2])));
                    }
                }
            }

            // the headset
            let hs = s(&st["audio"], &["headset"]);
            headset_row.set_visible(!hs.is_empty());
            headset.set(hs == "handsfree", "", if hs == "handsfree" { "Handsfree, with the mic" } else { "Headphones" });
        }
    };
    let draw: Rc<dyn Fn()> = Rc::new(draw);
    let d2 = draw.clone();
    *redraw.borrow_mut() = Some(Box::new(move || d2()));
    hub.on(move |v| {
        *state.borrow_mut() = v.clone();
        draw();
    });

    Rc::new(Panel { popup, menus })
}
