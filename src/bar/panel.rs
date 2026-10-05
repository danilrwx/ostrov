//! A panel in the bar (cc/): its face, its widgets' badges, and the panel unrolled out of it on a click. Its
//! commands: ostrov panel.ID menu NAME, settings [SECTION], appearance. One panel in every bar: the first bar's
//! builds it, its face drawn there; the other bars' show that face as it is drawn, the panel moving under the one
//! clicked.

use std::rc::Rc;

use gtk4::prelude::*;

use super::{pill, slot, Block, Ctx};
use crate::popup::Side;

const FORMS: &[&str] = &["menu NAME", "settings [SECTION]", "appearance", "size NAME W H", "cols N", "edit [NAME]"];

pub fn build(cx: &Rc<Ctx>, id: &str, side: Side) -> Block {
    let face = pill();
    let s = slot(&face);
    let s2 = s.clone();
    let panel = match crate::cc::panel(id) {
        Some(p) => {
            let mirror = gtk4::Picture::for_paintable(&gtk4::WidgetPaintable::new(Some(&p.face)));
            mirror.set_can_shrink(false);
            face.append(&mirror);
            p
        }
        None => {
            // the badges in a box of their own, the one the other bars show
            let inner = pill();
            inner.remove_css_class("pill");
            face.append(&inner);
            crate::cc::build(&cx.host, &cx.hub, &s, side, crate::cc::Spec::of(id), &inner)
        }
    };
    panel.popup.add_tab(&cx.host, &s);
    let click = gtk4::GestureClick::new();
    let (p, h) = (panel.clone(), Rc::downgrade(&cx.host));
    click.connect_released(move |_, _, _, _| {
        if let Some(h) = h.upgrade() {
            p.popup.toggle_at(&h, &s2);
        }
    });
    face.add_controller(click);
    face.set_cursor_from_name(Some("pointer"));

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
            // a widget sized on the grid, as the inspector's chips do: from a script, a test
            ["size", key, w, h] => {
                let (Ok(w), Ok(h)) = (w.parse(), h.parse()) else { return Err(crate::forms::usage("panel.ID", FORMS)) };
                p.resize(key, w, h)?;
                Ok(String::new())
            }
            // its grid's width in cells, as Edit's Width sets it
            ["cols", n] => {
                p.set_cols(n.parse().map_err(|_| crate::forms::usage("panel.ID", FORMS))?);
                Ok(String::new())
            }
            // in the editing, the widget picked: its inspector, its settings
            ["edit", key @ ..] => {
                p.edit(key.first().copied().unwrap_or(""));
                Ok(String::new())
            }
            ["appearance"] => {
                p.open_page("appearance", None);
                Ok(String::new())
            }
            _ => Err(crate::forms::usage("panel.ID", FORMS)),
        })),
        forms: FORMS,
        ..Block::new(&s)
    }
}
