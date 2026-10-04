use gtk4::prelude::*;
use gtk4::Orientation;

use crate::cc::{Ctx, Widget};
use crate::hub::{s, service};
use crate::settings::{Field, Kind, Schema, Section};
use crate::style::clear;
use crate::ui::{menu, row, Memo, Toggle};

/// The wallpaper's toggle: on (the picture) or off (a plain ground); its menu a random one and the pictures.
pub fn wallpaper(c: &Ctx) -> Widget {
    let st = c.state.clone();
    let t = Toggle::new("preferences-desktop-wallpaper-symbolic", "Wallpaper", move || {
        let on = st.borrow()["wallpaper"]["on"].as_bool().unwrap_or(false);
        service(&["wallpaper", if on { "off" } else { "on" }]);
    }, Some(c.flip.clone()));
    let (card, items) = menu("preferences-desktop-wallpaper-symbolic", "Wallpaper");
    let memo = Memo::default();
    let t2 = t.clone();
    Widget::toggle(&t, Some(&card), move |st| {
        let w = &st["wallpaper"];
        let on = w["on"].as_bool().unwrap_or(false);
        let name = s(w, &["path"]).rsplit('/').next().unwrap_or("").to_string();
        t2.set(on, "", if on { &name } else { "" });
        if !memo.changed("wallpaper", w.to_string()) {
            return;
        }
        clear(&items);
        items.append(&row("", "Random", "", false, || service(&["wallpaper", "random"])));
        items.append(&gtk4::Separator::new(Orientation::Horizontal));
        for p in w["wallpapers"].as_array().into_iter().flatten().filter_map(|p| p.as_str()) {
            let name = p.rsplit('/').next().unwrap_or("").to_string();
            let picked = on && p == s(w, &["path"]);
            let p = p.to_string();
            items.append(&row("", &name, "", picked, move || service(&["wallpaper", "set", &p])));
        }
    })
}

/// [widget.wallpaper]: where the pictures are, how see-through the bar is over one, what follows a pick.
pub fn wallpaper_settings() -> Schema {
    let dir = Field::new("dir", "Pictures", Kind::Path)
        .default("~/Pictures/wallpapers")
        .help("The directory whose JPEG, PNG and WebP pictures the menu lists.");
    let bar = Field::new("bar", "Bar over a picture", Kind::Number { min: 0.0, max: 1.0, step: 0.05, slider: true })
        .default(0.65)
        .help("How solid the bar's black is over the wallpaper: 1 solid, 0 clear. Over no picture it is solid.");
    let hook = Field::new("on_change", "On a change, run", Kind::String)
        .help("A command run after every pick, the picture in $OSTROV_WALLPAPER (empty: none), for what else follows it.");
    Schema { sections: vec![Section::new("", "Wallpaper", vec![dir, bar, hook])] }
}
