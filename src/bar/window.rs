//! The window focused: its app's icon and its title, as the compositor says on every change.

use std::rc::Rc;

use gtk4::prelude::*;

use super::{pill, slot, Block, Ctx};
use crate::wm::Event;

pub fn build(cx: &Rc<Ctx>) -> Block {
    let p = pill();
    let icon = gtk4::Image::new();
    let title = gtk4::Label::new(None);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.set_max_width_chars(48);
    p.append(&icon);
    p.append(&title);
    p.set_visible(false);
    let s = slot(&p);
    cx.on_wm(move |e| {
        let Event::Window(class, t) = e else { return };
        p.set_visible(!class.is_empty() || !t.is_empty());
        match crate::switcher::icon(class) {
            Some(g) => icon.set_from_gicon(&g),
            None => icon.set_icon_name(Some("application-x-executable-symbolic")),
        }
        title.set_text(t);
        p.set_tooltip_text(Some(t));
    });
    Block::new(&s)
}
