use gtk4::prelude::*;

use crate::cc::{Ctx, Face, Widget};
use crate::hub::service;
use crate::i18n::t;
use crate::ui::{arrow, menu, Slider};

/// Brightness, a slider; with a plugin's widget going inside it (the night light's), an arrow and that in its
/// menu.
pub fn brightness(c: &Ctx) -> Widget {
    let bri = Slider::new("display-brightness-symbolic", |v| service(&["brightness", &format!("{}", (v * 100.0).round() as i64)]));
    let inside = crate::plugins::attached("brightness", c);
    let card = (!inside.is_empty()).then(|| {
        bri.root.append(&arrow(c.flip.clone()));
        let (card, items) = menu("display-brightness-symbolic", t("Brightness"));
        for w in &inside {
            items.append(w);
        }
        card
    });
    let root = bri.root.clone();
    let face = Face::new(&bri.face);
    let w = Widget::new(&root, card.as_ref(), move |st| {
        bri.set(st["brightness"].as_f64().unwrap_or(0.0) / 100.0, "display-brightness-symbolic");
    });
    Widget { face: Some(face), ..w }
}
