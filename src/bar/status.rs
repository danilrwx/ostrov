//! The status: Wi-Fi, the volume, the battery; a click unrolls the quick settings out of it (cc/). Its
//! commands: ostrov status menu NAME, the quick settings with that menu unfolded.

use std::rc::Rc;

use gtk4::prelude::*;

use super::{pill, slot, Block, Ctx};

fn volume_icon(v: f64, muted: bool) -> &'static str {
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

const FORMS: &[&str] = &["menu NAME", "settings [SECTION]", "appearance"];

pub fn build(cx: &Rc<Ctx>) -> Block {
    let status = pill();
    let wifi = gtk4::Image::from_icon_name("network-wireless-offline-symbolic");
    let vol = gtk4::Image::from_icon_name("audio-volume-high-symbolic");
    let bat = gtk4::Image::from_icon_name("battery-missing-symbolic");
    status.append(&wifi);
    status.append(&vol);
    status.append(&bat);
    let s = slot(&status);

    cx.hub.on(move |st| {
        let w = &st["wifi"];
        let icon = if !w["on"].as_bool().unwrap_or(false) {
            "network-wireless-disabled-symbolic".to_string()
        } else if w["ssid"].as_str().unwrap_or("").is_empty() {
            "network-wireless-offline-symbolic".to_string()
        } else {
            let bars = ["none", "weak", "ok", "good", "excellent"];
            format!("network-wireless-signal-{}-symbolic", bars[w["signal"].as_u64().unwrap_or(0).min(4) as usize])
        };
        wifi.set_icon_name(Some(&icon));
        let a = &st["audio"];
        vol.set_icon_name(Some(volume_icon(a["volume"].as_f64().unwrap_or(0.0), a["muted"].as_bool().unwrap_or(false))));
        let b = &st["battery"];
        bat.set_visible(b["present"].as_bool().unwrap_or(false));
        bat.set_icon_name(b["icon"].as_str());
        let t = crate::ui::battery_time(b);
        let t = if t.is_empty() { t } else { format!(", {t}") };
        bat.set_tooltip_text(Some(&format!("{}%{t}", b["percent"].as_f64().unwrap_or(0.0).round())));
    });

    let panel = crate::cc::build(&cx.host, &cx.hub, &s);
    let click = gtk4::GestureClick::new();
    let p = panel.clone();
    click.connect_released(move |_, _, _, _| p.toggle());
    status.add_controller(click);
    status.set_cursor_from_name(Some("pointer"));

    let p = panel.clone();
    Block {
        popup: Some(panel.popup.clone()),
        command: Some(Box::new(move |args| match args {
            ["menu", name] => {
                p.open_menu(name);
                Ok(String::new())
            }
            ["settings", entry @ ..] => {
                p.open_page("settings", entry.first().copied());
                Ok(String::new())
            }
            ["appearance"] => {
                p.open_page("appearance", None);
                Ok(String::new())
            }
            _ => Err(crate::forms::usage("status", FORMS)),
        })),
        forms: FORMS,
        ..Block::new(&s)
    }
}
