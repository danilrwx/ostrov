//! The keyboard layout, US or RU, as the compositor names it.

use std::rc::Rc;

use gtk4::prelude::*;

use super::{slot, Block, Ctx};

pub fn build(cx: &Rc<Ctx>) -> Block {
    let layout = gtk4::Label::new(Some("US"));
    layout.add_css_class("pill");
    let s = slot(&layout);
    cx.hub.on(move |st| {
        layout.set_text(if st["keymap"].as_str().unwrap_or("").contains("Russian") { "RU" } else { "US" });
    });
    Block::new(&s)
}
