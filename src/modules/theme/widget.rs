
use gtk4::prelude::*;
use gtk4::{glib, Orientation};

use crate::cc::{Ctx, Widget};
use crate::hub::{bin, run, s};
use crate::settings::{Field, Kind, Schema, Section};
use crate::style::clear;
use crate::ui::{menu, row, Memo, Toggle};

pub fn wallpaper(c: &Ctx) -> Widget {
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

/// [widget.wallpaper]: where the wallpapers are (services/theme.rs lists them).
pub fn wallpaper_settings() -> Schema {
    let dir = Field::new("dir", "Wallpapers", Kind::Path)
        .default("~/Pictures/wallpapers")
        .help("The directory whose JPEG and PNG pictures the menu lists.");
    Schema { sections: vec![Section::new("", "Wallpaper", vec![dir])] }
}
