use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::Orientation;
use serde_json::Value;

use crate::cc::{Ctx, Widget};
use crate::hub::{s, service, service_then};
use crate::style::{clear, label};
use crate::ui::{menu, on_right_click, row, Memo, Toggle};

pub fn bluetooth(c: &Ctx) -> Widget {
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
