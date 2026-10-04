
use gtk4::prelude::*;
use gtk4::Orientation;

use crate::cc::{Ctx, Widget};
use crate::hub::{bin, run, s};
use crate::style::clear;
use crate::ui::{menu, row, Memo, Toggle};

pub fn openvpn(c: &Ctx) -> Widget {
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

pub fn vless(c: &Ctx) -> Widget {
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
