use crate::cc::{Ctx, Face, Widget};
use crate::hub::service;
use crate::ui::Slider;

/// Brightness, a slider (the night light is the plugin night's).
pub fn brightness(_: &Ctx) -> Widget {
    let bri = Slider::new("display-brightness-symbolic", |v| service(&["brightness", &format!("{}", (v * 100.0).round() as i64)]));
    let root = bri.root.clone();
    let face = Face::new(&bri.face);
    let w = Widget::new(&root, None, move |st| {
        bri.set(st["brightness"].as_f64().unwrap_or(0.0) / 100.0, "display-brightness-symbolic");
    });
    Widget { face: Some(face), ..w }
}
