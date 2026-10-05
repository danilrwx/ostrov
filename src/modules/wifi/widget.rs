use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Orientation};

use crate::cc::{Ctx, Widget};
use crate::hub::{s, service, service_then};
use crate::i18n::t;
use crate::style::{clear, label};
use crate::ui::{menu, on_right_click, row, Memo, Toggle};

const BARS: [&str; 5] = ["none", "weak", "ok", "good", "excellent"];

pub fn wifi(c: &Ctx) -> Widget {
    let st = c.state.clone();
    let tg = Toggle::new("network-wireless-symbolic", t("Wi-Fi"), move || {
        let on = st.borrow()["wifi"]["on"].as_bool().unwrap_or(false);
        service(&["wifi", if on { "off" } else { "on" }]);
    }, Some(c.flip.clone()));
    let (card, items) = menu("network-wireless-symbolic", t("Wi-Fi"));
    // the network whose passphrase is asked for, and what went wrong joining it, kept across redraws
    let asking: Rc<RefCell<String>> = Rc::default();
    let error: Rc<RefCell<String>> = Rc::default();
    // a known network that would not join, and why: forgotten and its passphrase asked for again from there
    let failed: Rc<RefCell<(String, String)>> = Rc::default();
    let memo = Memo::default();
    let again = c.again.clone();
    let t2 = tg.clone();
    Widget::toggle(&tg, Some(&card), move |st| {
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
        let key = if asking.borrow().is_empty() {
            format!("{}{:?}", w["networks"], failed.borrow())
        } else {
            format!("{}{}", asking.borrow(), error.borrow())
        };
        if !memo.changed("wifi", key) {
            return;
        }
        clear(&items);
        for net in w["networks"].as_array().into_iter().flatten() {
            let ssid = s(net, &["ssid"]).to_string();
            let known = net["known"].as_bool().unwrap_or(false) || s(net, &["security"]) == "open";
            let icon = format!("network-wireless-signal-{}-symbolic", BARS[net["signal"].as_u64().unwrap_or(0).min(4) as usize]);
            let (ask, err, again2, ssid2, fail) = (asking.clone(), error.clone(), again.clone(), ssid.clone(), failed.clone());
            let r = row(&icon, &ssid, if known { "" } else { "🔒" }, net["connected"].as_bool().unwrap_or(false), move || {
                if known {
                    fail.borrow_mut().0.clear();
                    let (fail, again, ssid) = (fail.clone(), again2.clone(), ssid2.clone());
                    service_then(vec!["wifi".into(), "connect".into(), ssid2.clone()], None, move |r| {
                        if let Err(e) = r {
                            *fail.borrow_mut() = (ssid.clone(), e);
                            again();
                        }
                    });
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
            if failed.borrow().0 == ssid && asking.borrow().is_empty() {
                rejoin(&items, &ssid, &failed.borrow().1, &asking, &failed, &again);
            }
            if *asking.borrow() == ssid {
                passphrase(&items, &ssid, &asking, &error, &again);
            }
        }
        items.append(&gtk4::Separator::new(Orientation::Horizontal));
        items.append(&row("", t("Scan"), "", false, || service(&["wifi", "scan"])));
    })
}

/// Under a known network that would not join: why, and its passphrase entered anew (it forgotten first). A
/// network's saved state can stop it joining (WPA3's, after the router's changed) where a new one goes through.
fn rejoin(items: &gtk4::Box, ssid: &str, why: &str, asking: &Rc<RefCell<String>>, failed: &Rc<RefCell<(String, String)>>, again: &Rc<dyn Fn()>) {
    // iwd's words without its error's name (net.connman.iwd.Failed: Operation failed)
    let why = why.rsplit(": ").next().unwrap_or(why);
    let note = label(&format!("{} {why}", t("Could not join.")), "error");
    note.set_wrap(true);
    note.set_margin_start(36);
    note.set_margin_end(10);
    items.append(&note);
    let b = crate::ui::chip(t("Forget and Enter the Passphrase"));
    b.set_halign(gtk4::Align::Start);
    b.set_margin_start(36);
    b.set_margin_bottom(4);
    let (ssid, ask, fail, again) = (ssid.to_string(), asking.clone(), failed.clone(), again.clone());
    b.connect_clicked(move |_| {
        let (ssid2, ask, fail, again) = (ssid.clone(), ask.clone(), fail.clone(), again.clone());
        service_then(vec!["wifi".into(), "forget".into(), ssid.clone()], None, move |r| {
            match r {
                Ok(()) => {
                    *ask.borrow_mut() = ssid2.clone();
                    fail.borrow_mut().0.clear();
                }
                Err(e) => fail.borrow_mut().1 = e,
            }
            again();
        });
    });
    items.append(&b);
}

/// The passphrase right under the network asked for: Enter or Connect joins.
fn passphrase(items: &gtk4::Box, ssid: &str, asking: &Rc<RefCell<String>>, error: &Rc<RefCell<String>>, again: &Rc<dyn Fn()>) {
    let bx = gtk4::Box::new(Orientation::Horizontal, 6);
    bx.set_margin_start(36);
    bx.set_margin_end(10);
    bx.set_margin_bottom(4);
    let pass = gtk4::PasswordEntry::new();
    pass.set_hexpand(true);
    let go = crate::ui::primary(t("Connect"));
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
            go.set_label(t("Joining…"));
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
