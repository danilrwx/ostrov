//! A panel in the bar (cc/): its face, its widgets' badges, and the panel unrolled out of it on a click. Its
//! commands: ostrov panel.ID menu NAME, settings [SECTION], appearance.

use std::rc::Rc;

use gtk4::prelude::*;

use super::{pill, slot, Block, Ctx};
use crate::popup::Side;

const FORMS: &[&str] = &["menu NAME", "settings [SECTION]", "appearance", "size NAME W H"];

pub fn build(cx: &Rc<Ctx>, id: &str, side: Side) -> Block {
    let face = pill();
    let s = slot(&face);
    let panel = crate::cc::build(&cx.host, &cx.hub, &s, side, crate::cc::Spec::of(id), &face);
    let click = gtk4::GestureClick::new();
    let p = panel.clone();
    click.connect_released(move |_, _, _, _| p.toggle());
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
