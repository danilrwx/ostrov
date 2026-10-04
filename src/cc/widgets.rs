//! The control centre's widgets, the registry the grid and its gallery choose from: what the quick settings had,
//! each on its own. The battery; buttons for a screenshot, the lock, the power menu; the sliders (volume with its
//! outputs and the apps playing, the mic with its inputs, brightness with the night light); the toggles: Wi-Fi
//! (a new network asks its passphrase right under it, a right click forgets a known one), Bluetooth (paired
//! devices, a scan, pairing), the power mode, the wallpaper, OpenVPN, VLESS, Keep Awake, the headset, Displays.
//! Everything from the services' state; the switching through them and the dotfiles' scripts.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};
use serde_json::Value;

use super::{Ctx, Meta, Widget};
use crate::hub::{bin, run, s, service, service_then};
use crate::style::{clear, label};
use crate::ui::{app_icon, arrow, battery_time, level_icon, menu, on_right_click, round, row, Memo, Slider, Toggle};

const TOGGLE: &[(u8, u8)] = &[(4, 1), (2, 1), (1, 1), (8, 1)];
const BUTTON: &[(u8, u8)] = &[(1, 1), (2, 1)];
const SLIDER: &[(u8, u8)] = &[(8, 1)];

pub fn all() -> Vec<Meta> {
    let m = |id, name, icon, sizes, make| Meta { id, name, icon, sizes, make };
    vec![
        m("battery", "Battery", "battery-good-symbolic", &[(5, 1), (2, 1), (3, 1), (4, 1), (8, 1)], battery),
        m("screenshot", "Screenshot", "applets-screenshooter-symbolic", BUTTON, screenshot),
        m("lock", "Lock", "system-lock-screen-symbolic", BUTTON, lock),
        m("session", "Power Off", "system-shutdown-symbolic", BUTTON, session),
        m("volume", "Volume", "audio-volume-high-symbolic", SLIDER, volume),
        m("mic", "Microphone", "microphone-sensitivity-high-symbolic", SLIDER, mic),
        m("brightness", "Brightness", "display-brightness-symbolic", SLIDER, brightness),
        m("wifi", "Wi-Fi", "network-wireless-symbolic", TOGGLE, wifi),
        m("bt", "Bluetooth", "bluetooth-active-symbolic", TOGGLE, bluetooth),
        m("power", "Power Mode", "power-profile-balanced-symbolic", TOGGLE, power),
        m("wallpaper", "Wallpaper", "preferences-desktop-wallpaper-symbolic", TOGGLE, wallpaper),
        m("openvpn", "OpenVPN", "network-vpn-symbolic", TOGGLE, openvpn),
        m("vless", "VLESS", "network-vpn-symbolic", TOGGLE, vless),
        m("awake", "Keep Awake", "weather-clear-symbolic", TOGGLE, awake),
        m("headset", "Headset", "audio-headphones-symbolic", TOGGLE, headset),
        m("displays", "Displays", "video-display-symbolic", TOGGLE, displays),
    ]
}

/// The battery: its icon and percent, from three cells what is left of it too.
fn battery(_: &Ctx) -> Widget {
    let bx = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    bx.add_css_class("battery");
    let icon = gtk4::Image::from_icon_name("battery-missing-symbolic");
    let pct = label("", "bold");
    let time = label("", "dim");
    time.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    bx.append(&icon);
    bx.append(&pct);
    bx.append(&time);
    let wide = Rc::new(std::cell::Cell::new(true));
    let (w2, t2, root) = (wide.clone(), time.clone(), bx.clone());
    let draw = move |st: &Value| {
        let b = &st["battery"];
        icon.set_icon_name(Some(s(b, &["icon"])));
        pct.set_text(&format!("{}%", b["percent"].as_f64().unwrap_or(0.0).round()));
        let t = battery_time(b);
        time.set_text(&t);
        time.set_visible(wide.get() && !t.is_empty());
        bx.set_visible(b["present"].as_bool().unwrap_or(false));
    };
    Widget {
        size: Box::new(move |w, _| {
            w2.set(w >= 3);
            t2.set_visible(w >= 3 && !t2.text().is_empty());
        }),
        ..Widget::new(&root, None, draw)
    }
}

fn screenshot(c: &Ctx) -> Widget {
    let close = c.close.clone();
    let b = round("applets-screenshooter-symbolic", move || {
        close();
        // once the panel has rolled up out of the picture
        let me = std::env::current_exe().unwrap_or_default();
        run(&["sh", "-c", "sleep 0.2; exec \"$0\" screenshot", &me.to_string_lossy()]);
    });
    b.set_tooltip_text(Some("Screenshot"));
    Widget::new(&b, None, |_| ())
}

fn lock(c: &Ctx) -> Widget {
    let close = c.close.clone();
    let b = round("system-lock-screen-symbolic", move || {
        close();
        run(&["loginctl", "lock-session"]);
    });
    b.set_tooltip_text(Some("Lock"));
    Widget::new(&b, None, |_| ())
}

/// The power menu's button: suspend, restart, power off, log out under it.
fn session(c: &Ctx) -> Widget {
    let flip = c.flip.clone();
    let b = round("system-shutdown-symbolic", move || flip());
    b.set_tooltip_text(Some("Power Off"));
    let (card, items) = menu("system-shutdown-symbolic", "Power Off");
    for (icon, text, cmd) in [
        ("weather-clear-night-symbolic", "Suspend", vec!["systemctl", "suspend"]),
        ("view-refresh-symbolic", "Restart…", vec!["systemctl", "reboot"]),
        ("system-shutdown-symbolic", "Power Off…", vec!["systemctl", "poweroff"]),
        ("system-log-out-symbolic", "Log Out", vec!["sh", "-c", "hyprctl dispatch exit || swaymsg exit"]),
    ] {
        let close = c.close.clone();
        items.append(&row(icon, text, "", false, move || {
            close();
            run(&cmd);
        }));
    }
    Widget::new(&b, Some(&card), |_| ())
}

/// The volume: the default output's, mute on its icon; its menu the outputs, then the apps playing, a slider
/// each.
fn volume(c: &Ctx) -> Widget {
    let vol = Slider::new("audio-volume-high-symbolic", |v| run(&["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{v:.2}")]));
    vol.icon.connect_clicked(|_| run(&["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]));
    vol.root.append(&arrow(c.flip.clone()));
    let (card, items) = menu("audio-speakers-symbolic", "Sound Output");
    let outs = gtk4::Box::new(Orientation::Vertical, 0);
    let apps = gtk4::Box::new(Orientation::Vertical, 4);
    items.append(&outs);
    items.append(&apps);
    let sliders: RefCell<Vec<Slider>> = RefCell::default();
    let memo = Memo::default();
    let root = vol.root.clone();
    Widget::new(&root, Some(&card), move |st| {
        let a = &st["audio"];
        let (v, m) = (a["volume"].as_f64().unwrap_or(0.0), a["muted"].as_bool().unwrap_or(false));
        vol.set(v, &level_icon("audio-volume", v, m));
        devices(&memo, "outs", &a["sinks"], &outs);
        let streams = a["streams"].as_array().cloned().unwrap_or_default();
        let who = |x: &Value| format!("{} {} {} {}", x["id"], x["name"], x["icon"], x["bin"]);
        if memo.changed("apps", streams.iter().map(who).collect()) {
            clear(&apps);
            let mut sliders = sliders.borrow_mut();
            sliders.clear();
            if !streams.is_empty() {
                apps.append(&gtk4::Separator::new(Orientation::Horizontal));
            }
            for x in &streams {
                let id = x["id"].to_string();
                let sl = Slider::new("", move |v| service(&["audio", "volume", &id, &format!("{v:.2}")]));
                sl.icon.set_child(Some(&app_icon(s(x, &["icon"]), s(x, &["bin"]), s(x, &["name"]))));
                sl.icon.set_can_target(false);
                let name = label(s(x, &["name"]), "dim");
                name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                apps.append(&name);
                apps.append(&sl.root);
                sliders.push(sl);
            }
        }
        for (sl, x) in sliders.borrow().iter().zip(&streams) {
            sl.set(x["volume"].as_f64().unwrap_or(0.0), "");
        }
    })
}

/// A list of sound devices, the default ticked, a click making one the default.
fn devices(memo: &Memo, key: &'static str, list: &Value, items: &gtk4::Box) {
    if !memo.changed(key, list.to_string()) {
        return;
    }
    clear(items);
    for d in list.as_array().into_iter().flatten() {
        let id = d["id"].to_string();
        items.append(&row("", s(d, &["name"]), "", d["def"].as_bool().unwrap_or(false), move || {
            run(&["wpctl", "set-default", &id])
        }));
    }
}

fn mic(c: &Ctx) -> Widget {
    let mic = Slider::new("microphone-sensitivity-high-symbolic", |v| run(&["wpctl", "set-volume", "@DEFAULT_AUDIO_SOURCE@", &format!("{v:.2}")]));
    mic.icon.connect_clicked(|_| run(&["wpctl", "set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"]));
    mic.root.append(&arrow(c.flip.clone()));
    let (card, items) = menu("audio-input-microphone-symbolic", "Sound Input");
    let memo = Memo::default();
    let root = mic.root.clone();
    Widget::new(&root, Some(&card), move |st| {
        let a = &st["audio"];
        let (v, m) = (a["mic"].as_f64().unwrap_or(0.0), a["micMuted"].as_bool().unwrap_or(false));
        mic.set(v, &level_icon("microphone-sensitivity", v, m));
        devices(&memo, "ins", &a["sources"], &items);
    })
}

/// Brightness, the night light behind its arrow: the mode, the hours, the warmth, shown on the screen while
/// dragged.
fn brightness(c: &Ctx) -> Widget {
    let bri = Slider::new("display-brightness-symbolic", |v| service(&["brightness", &format!("{}", (v * 100.0).round() as i64)]));
    bri.root.append(&arrow(c.flip.clone()));
    let (card, items) = menu("night-light-symbolic", "Night Light");
    let modes = gtk4::Box::new(Orientation::Vertical, 0);
    items.append(&modes);
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
    let set_hours = Rc::new(move || service(&["night", "time", &f2.text(), &t2.text()]));
    let s1 = set_hours.clone();
    from.connect_activate(move |_| s1());
    to.connect_activate(move |_| set_hours());
    items.append(&hours);
    let warm_head = gtk4::Box::new(Orientation::Horizontal, 0);
    warm_head.set_margin_top(6);
    let wl = label("Warmth", "dim");
    wl.set_hexpand(true);
    let kelvin = label("", "dim");
    warm_head.append(&wl);
    warm_head.append(&kelvin);
    items.append(&warm_head);
    let save: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let k2 = kelvin.clone();
    let warm = Slider::new("night-light-symbolic", move |v| {
        let k = (6500.0 - v * 4000.0).round() as i64;
        k2.set_text(&format!("{k} K"));
        service(&["night", "preview", &k.to_string()]);
        // kept once the slider rests
        if let Some(id) = save.borrow_mut().take() {
            id.remove();
        }
        let s2 = save.clone();
        *save.borrow_mut() = Some(glib::timeout_add_local_once(Duration::from_millis(500), move || {
            s2.borrow_mut().take();
            service(&["night", "temp", &k.to_string()]);
        }));
    });
    items.append(&warm.root);
    let memo = Memo::default();
    let root = bri.root.clone();
    Widget::new(&root, Some(&card), move |st| {
        bri.set(st["brightness"].as_f64().unwrap_or(0.0) / 100.0, "display-brightness-symbolic");
        let n = &st["night"];
        if !memo.changed("night", n.to_string()) {
            return;
        }
        clear(&modes);
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
            modes.append(&row("", text, &note, mode == m, move || service(&["night", "mode", m])));
        }
        hours.set_visible(mode == "time");
        from.set_text(s(n, &["from"]));
        to.set_text(s(n, &["to"]));
        let k = n["temp"].as_f64().unwrap_or(4000.0);
        warm.set((6500.0 - k) / 4000.0, "night-light-symbolic");
        kelvin.set_text(&format!("{k} K"));
    })
}

const BARS: [&str; 5] = ["none", "weak", "ok", "good", "excellent"];

fn wifi(c: &Ctx) -> Widget {
    let st = c.state.clone();
    let t = Toggle::new("network-wireless-symbolic", "Wi-Fi", move || {
        let on = st.borrow()["wifi"]["on"].as_bool().unwrap_or(false);
        service(&["wifi", if on { "off" } else { "on" }]);
    }, Some(c.flip.clone()));
    let (card, items) = menu("network-wireless-symbolic", "Wi-Fi");
    // the network whose passphrase is asked for, and what went wrong joining it, kept across redraws
    let asking: Rc<RefCell<String>> = Rc::default();
    let error: Rc<RefCell<String>> = Rc::default();
    let memo = Memo::default();
    let again = c.again.clone();
    let t2 = t.clone();
    Widget::toggle(&t, Some(&card), move |st| {
        let w = &st["wifi"];
        let on = w["on"].as_bool().unwrap_or(false);
        let icon = if !on {
            "network-wireless-disabled-symbolic".to_string()
        } else if s(w, &["ssid"]).is_empty() {
            "network-wireless-offline-symbolic".to_string()
        } else {
            format!("network-wireless-signal-{}-symbolic", BARS[w["signal"].as_u64().unwrap_or(0).min(4) as usize])
        };
        t2.set(on, &icon, if on { s(w, &["ssid"]) } else { "" });
        // the network asked for joined (the state says so before the connect returns, iwd joining it as soon as
        // it has the passphrase): the passphrase is done with
        if !asking.borrow().is_empty()
            && w["networks"].as_array().into_iter().flatten().any(|n| {
                n["connected"].as_bool().unwrap_or(false) && s(n, &["ssid"]) == asking.borrow().as_str()
            })
        {
            asking.borrow_mut().clear();
            error.borrow_mut().clear();
        }
        // while a passphrase is typed the list holds still, a redraw would drop what is typed
        let key = if asking.borrow().is_empty() { w["networks"].to_string() } else { format!("{}{}", asking.borrow(), error.borrow()) };
        if !memo.changed("wifi", key) {
            return;
        }
        clear(&items);
        for net in w["networks"].as_array().into_iter().flatten() {
            let ssid = s(net, &["ssid"]).to_string();
            let known = net["known"].as_bool().unwrap_or(false) || s(net, &["security"]) == "open";
            let icon = format!("network-wireless-signal-{}-symbolic", BARS[net["signal"].as_u64().unwrap_or(0).min(4) as usize]);
            let (ask, err, again2, ssid2) = (asking.clone(), error.clone(), again.clone(), ssid.clone());
            let r = row(&icon, &ssid, if known { "" } else { "🔒" }, net["connected"].as_bool().unwrap_or(false), move || {
                if known {
                    service(&["wifi", "connect", &ssid2]);
                } else {
                    let now = if *ask.borrow() == ssid2 { String::new() } else { ssid2.clone() };
                    *ask.borrow_mut() = now;
                    err.borrow_mut().clear();
                    again2();
                }
            });
            if net["known"].as_bool().unwrap_or(false) {
                let ssid3 = ssid.clone();
                on_right_click(&r, move || service(&["wifi", "forget", &ssid3]));
            }
            items.append(&r);
            if *asking.borrow() == ssid {
                passphrase(&items, &ssid, &asking, &error, &again);
            }
        }
        items.append(&gtk4::Separator::new(Orientation::Horizontal));
        items.append(&row("", "Scan", "", false, || service(&["wifi", "scan"])));
    })
}

/// The passphrase right under the network asked for: Enter or Connect joins.
fn passphrase(items: &gtk4::Box, ssid: &str, asking: &Rc<RefCell<String>>, error: &Rc<RefCell<String>>, again: &Rc<dyn Fn()>) {
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
    items.append(&bx);
    let note = label(&error.borrow(), "error");
    note.set_margin_start(36);
    note.set_visible(!error.borrow().is_empty());
    items.append(&note);
    let join = {
        let (pass, go, ask, err, again, ssid) = (pass.clone(), go.clone(), asking.clone(), error.clone(), again.clone(), ssid.to_string());
        Rc::new(move || {
            let p = pass.text().to_string();
            if p.is_empty() {
                return;
            }
            go.set_label("Joining…");
            go.set_sensitive(false);
            let (ask, err, again) = (ask.clone(), err.clone(), again.clone());
            service_then(vec!["wifi".into(), "connect".into(), ssid.clone()], Some(p), move |r| {
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
    glib::idle_add_local_once(move || {
        pass.grab_focus();
    });
}

fn bluetooth(c: &Ctx) -> Widget {
    let st = c.state.clone();
    let t = Toggle::new("bluetooth-active-symbolic", "Bluetooth", move || {
        let on = st.borrow()["bt"]["on"].as_bool().unwrap_or(false);
        service(&["bt", if on { "off" } else { "on" }]);
    }, Some(c.flip.clone()));
    let (card, items) = menu("bluetooth-active-symbolic", "Bluetooth");
    // the device being paired, what went wrong pairing, a scan running, kept across redraws
    let pairing: Rc<RefCell<String>> = Rc::default();
    let error: Rc<RefCell<String>> = Rc::default();
    let scanning = Rc::new(RefCell::new(false));
    let memo = Memo::default();
    let again = c.again.clone();
    let t2 = t.clone();
    Widget::toggle(&t, Some(&card), move |st| {
        let b = &st["bt"];
        let on = b["on"].as_bool().unwrap_or(false);
        t2.set(on, if on { "bluetooth-active-symbolic" } else { "bluetooth-disabled-symbolic" }, s(b, &["connected"]));
        let key = format!("{}{}{}{}{}", b["devices"], b["discovering"], pairing.borrow(), error.borrow(), scanning.borrow());
        if !memo.changed("bt", key) {
            return;
        }
        clear(&items);
        let icon = |d: &Value| format!("{}-symbolic", if s(d, &["icon"]).is_empty() { "bluetooth" } else { s(d, &["icon"]) });
        let devs: Vec<&Value> = b["devices"].as_array().into_iter().flatten().collect();
        for d in devs.iter().filter(|d| d["paired"].as_bool().unwrap_or(false)) {
            let (addr, on) = (s(d, &["address"]).to_string(), d["connected"].as_bool().unwrap_or(false));
            let a2 = addr.clone();
            let r = row(&icon(d), s(d, &["name"]), "", on, move || service(&["bt", if on { "disconnect" } else { "connect" }, &a2]));
            on_right_click(&r, move || service(&["bt", "forget", &addr]));
            items.append(&r);
        }
        items.append(&gtk4::Separator::new(Orientation::Horizontal));
        let busy = b["discovering"].as_bool().unwrap_or(false) || *scanning.borrow();
        let (sc, again2) = (scanning.clone(), again.clone());
        items.append(&row("system-search-symbolic", if busy { "Scanning…" } else { "Scan for Devices" }, "", busy, move || {
            if *sc.borrow() {
                return;
            }
            *sc.borrow_mut() = true;
            let (sc, again) = (sc.clone(), again2.clone());
            service_then(vec!["bt".into(), "scan".into()], None, move |_| {
                *sc.borrow_mut() = false;
                again();
            });
            again2();
        }));
        for d in devs.iter().filter(|d| !d["paired"].as_bool().unwrap_or(false)) {
            let addr = s(d, &["address"]).to_string();
            let note = if *pairing.borrow() == addr { "pairing…" } else { "pair" };
            let (pa, err, again2) = (pairing.clone(), error.clone(), again.clone());
            items.append(&row(&icon(d), s(d, &["name"]), note, false, move || {
                if !pa.borrow().is_empty() {
                    return;
                }
                *pa.borrow_mut() = addr.clone();
                err.borrow_mut().clear();
                let (pa, err, again) = (pa.clone(), err.clone(), again2.clone());
                service_then(vec!["bt".into(), "pair".into(), addr.clone()], None, move |r| {
                    pa.borrow_mut().clear();
                    if let Err(e) = r {
                        *err.borrow_mut() = e;
                    }
                    again();
                });
                again2();
            }));
        }
        if !error.borrow().is_empty() {
            items.append(&label(&error.borrow(), "error"));
        }
    })
}

fn power(c: &Ctx) -> Widget {
    let st = c.state.clone();
    let t = Toggle::new("power-profile-balanced-symbolic", "Power Mode", move || {
        let order = ["power-saver", "balanced", "performance"];
        let cur = st.borrow()["power"]["active"].as_str().unwrap_or("balanced").to_string();
        let i = order.iter().position(|p| *p == cur).unwrap_or(1);
        service(&["power", "set", order[(i + 1) % 3]]);
    }, Some(c.flip.clone()));
    let (card, items) = menu("power-profile-balanced-symbolic", "Power Mode");
    let names = |p: &str| match p {
        "power-saver" => "Power Saver",
        "performance" => "Performance",
        _ => "Balanced",
    };
    let memo = Memo::default();
    let t2 = t.clone();
    Widget::toggle(&t, Some(&card), move |st| {
        let pw = s(&st["power"], &["active"]);
        let icon = format!("power-profile-{}-symbolic", if pw.is_empty() { "balanced" } else { pw });
        t2.set(pw != "balanced" && !pw.is_empty(), &icon, names(pw));
        if memo.changed("power", pw.to_string()) {
            clear(&items);
            for p in ["performance", "balanced", "power-saver"] {
                items.append(&row(&format!("power-profile-{p}-symbolic"), names(p), "", pw == p, move || service(&["power", "set", p])));
            }
        }
    })
}

fn wallpaper(c: &Ctx) -> Widget {
    let st = c.state.clone();
    let t = Toggle::new("preferences-desktop-wallpaper-symbolic", "Wallpaper", move || {
        let on = st.borrow()["theme"]["mode"].as_str() == Some("dark-wall");
        run(&[&bin("theme"), if on { "dark" } else { "dark-wall" }]);
    }, Some(c.flip.clone()));
    let (card, items) = menu("preferences-desktop-wallpaper-symbolic", "Wallpaper");
    let memo = Memo::default();
    let t2 = t.clone();
    Widget::toggle(&t, Some(&card), move |st| {
        let th = &st["theme"];
        let on = s(th, &["mode"]) == "dark-wall";
        let name = s(th, &["wallpaper"]).rsplit('/').next().unwrap_or("").to_string();
        t2.set(on, "", if on { &name } else { "" });
        if !memo.changed("theme", th.to_string()) {
            return;
        }
        clear(&items);
        let walls: Vec<String> = th["wallpapers"].as_array().into_iter().flatten().filter_map(|w| w.as_str().map(String::from)).collect();
        let (cur, ws2) = (s(th, &["wallpaper"]).to_string(), walls.clone());
        items.append(&row("", "Random", "", false, move || {
            let others: Vec<&String> = ws2.iter().filter(|w| **w != cur).collect();
            if !others.is_empty() {
                let i = (glib::random_int() as usize) % others.len();
                run(&[&bin("theme"), "dark-wall", others[i]]);
            }
        }));
        items.append(&gtk4::Separator::new(Orientation::Horizontal));
        for w in walls {
            let name = w.rsplit('/').next().unwrap_or("").to_string();
            let on = on && w == s(th, &["wallpaper"]);
            items.append(&row("", &name, "", on, move || run(&[&bin("theme"), "dark-wall", &w])));
        }
    })
}

fn openvpn(c: &Ctx) -> Widget {
    let st = c.state.clone();
    let t = Toggle::new("network-vpn-symbolic", "OpenVPN", move || {
        let o = &st.borrow()["openvpn"];
        if s(o, &["profile"]).is_empty() {
            if let Some(first) = o["profiles"][0].as_str() {
                run(&[&bin("openvpn-ctl"), "on", first]);
            }
        } else {
            run(&[&bin("openvpn-ctl"), "off"]);
        }
    }, Some(c.flip.clone()));
    let (card, items) = menu("network-vpn-symbolic", "OpenVPN");
    let memo = Memo::default();
    let t2 = t.clone();
    Widget::toggle(&t, Some(&card), move |st| {
        let o = &st["openvpn"];
        let prof = s(o, &["profile"]).to_string();
        let ready = o["ready"].as_bool().unwrap_or(false);
        let sub = if prof.is_empty() { String::new() } else if ready { prof.clone() } else { format!("{prof}, connecting") };
        t2.set(!prof.is_empty(), "", &sub);
        if !memo.changed("openvpn", o.to_string()) {
            return;
        }
        clear(&items);
        for p in o["profiles"].as_array().into_iter().flatten().filter_map(|p| p.as_str()) {
            let p = p.to_string();
            let note = if p == prof && !ready { "connecting" } else { "" };
            let p2 = p.clone();
            items.append(&row("", &p, note, p == prof, move || run(&[&bin("openvpn-ctl"), "on", &p2])));
        }
        items.append(&gtk4::Separator::new(Orientation::Horizontal));
        items.append(&row("", "Off", "", prof.is_empty(), || run(&[&bin("openvpn-ctl"), "off"])));
    })
}

fn vless(c: &Ctx) -> Widget {
    let st = c.state.clone();
    let t = Toggle::new("network-vpn-symbolic", "VLESS", move || {
        let on = s(&st.borrow()["vless"], &["how"]) != "off";
        if on { run(&[&bin("vless"), "off"]) } else { run(&[&bin("vless"), "on", "tun"]) }
    }, Some(c.flip.clone()));
    let (card, items) = menu("network-vpn-symbolic", "VLESS");
    let memo = Memo::default();
    let t2 = t.clone();
    Widget::toggle(&t, Some(&card), move |st| {
        let v = &st["vless"];
        let how = s(v, &["how"]);
        let on = how != "off" && !how.is_empty();
        let sub = if !on {
            String::new()
        } else if s(v, &["mode"]) == "global" {
            format!("{}, global", s(v, &["profile"]))
        } else {
            s(v, &["profile"]).to_string()
        };
        t2.set(on, "", &sub);
        if !memo.changed("vless", v.to_string()) {
            return;
        }
        clear(&items);
        items.append(&row("", "TUN, every app", "", how == "tun", || run(&[&bin("vless"), "on", "tun"])));
        items.append(&row("", "System proxy", "", how == "proxy", || run(&[&bin("vless"), "on", "proxy"])));
        if on {
            items.append(&gtk4::Separator::new(Orientation::Horizontal));
            let mode = s(v, &["mode"]);
            items.append(&row("", "Rule: Russia direct", "", mode == "rule", || run(&[&bin("vless"), "mode", "rule"])));
            items.append(&row("", "Global", "", mode == "global", || run(&[&bin("vless"), "mode", "global"])));
            items.append(&gtk4::Separator::new(Orientation::Horizontal));
            let cur = s(v, &["profile"]).to_string();
            for p in v["profiles"].as_array().into_iter().flatten().filter_map(|p| p.as_str()) {
                let p = p.to_string();
                let p2 = p.clone();
                items.append(&row("", &p, "", p == cur, move || run(&[&bin("vless"), "profile", &p2])));
            }
        }
    })
}

/// Keep Awake: idle neither locks nor turns the screens off while it is on (idle.rs).
fn awake(c: &Ctx) -> Widget {
    let again = c.again.clone();
    let t = Toggle::new("weather-clear-symbolic", "Keep Awake", move || {
        crate::idle::set_awake(!crate::idle::awake());
        again();
    }, None);
    let t2 = t.clone();
    Widget::toggle(&t, None, move |_| {
        let on = crate::idle::awake();
        t2.set(on, "", if on { "the screen stays on" } else { "" });
    })
}

/// The headset's mode, handsfree (with its mic) or headphones; there only while a headset is.
fn headset(_: &Ctx) -> Widget {
    let t = Toggle::new("audio-headphones-symbolic", "Headset", || service(&["headset"]), None);
    let t2 = t.clone();
    Widget::toggle(&t, None, move |st| {
        let hs = s(&st["audio"], &["headset"]);
        t2.root.set_visible(!hs.is_empty());
        t2.set(hs == "handsfree", "", if hs == "handsfree" { "Handsfree, with the mic" } else { "Headphones" });
    })
}

fn displays(c: &Ctx) -> Widget {
    let t = Toggle::new("video-display-symbolic", "Displays", {
        let flip = c.flip.clone();
        move || flip()
    }, Some(c.flip.clone()));
    let (card, items) = menu("video-display-symbolic", "Displays");
    let memo = Memo::default();
    let t2 = t.clone();
    Widget::toggle(&t, Some(&card), move |st| {
        let d = &st["displays"];
        super::displays::draw(d, &t2, &items, memo.changed("displays", d.to_string()));
    })
}
