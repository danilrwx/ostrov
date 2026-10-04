
use gtk4::prelude::*;

use crate::cc::{Ctx, Widget};
use crate::hub::{s, service};
use crate::style::clear;
use crate::ui::{menu, row, Memo, Toggle};

pub fn power(c: &Ctx) -> Widget {
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
