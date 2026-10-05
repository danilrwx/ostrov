//! A panel's widget standing in the bar on its own, widget.ID: its badge (its icon if it has none) as the
//! block, its menu unrolled out of it on a click (a toggle's arrow, a slider's). Any widget there is: ostrov's
//! own, a KDL file's, a plugin's. A plugin's badge saying "click" has a left click of its own, sent to the plugin
//! (a recording stopped), the menu on a right click.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use serde_json::Value;

use super::{pill, slot, Block, Ctx};
use crate::popup::{Popup, Side};

pub fn build(cx: &Rc<Ctx>, id: &str, side: Side) -> Option<Block> {
    let reg = crate::cc::registry();
    let Some(m) = reg.iter().find(|m| m.id == id) else {
        eprintln!("ostrov: no widget {id}");
        return None;
    };
    let face = pill();
    let s = slot(&face);
    let popup: Rc<RefCell<Option<Rc<Popup>>>> = Rc::default();
    let state = Rc::new(RefCell::new(Value::Null));
    let flip: Rc<dyn Fn()> = {
        let popup = popup.clone();
        Rc::new(move || {
            if let Some(p) = popup.borrow().as_ref() {
                p.toggle();
            }
        })
    };
    let close: Rc<dyn Fn()> = {
        let popup = popup.clone();
        Rc::new(move || {
            if let Some(p) = popup.borrow().as_ref() {
                p.close();
            }
        })
    };
    let w = Rc::new(RefCell::new(None::<crate::cc::Widget>));
    let again: Rc<dyn Fn()> = {
        let (w, state) = (w.clone(), state.clone());
        Rc::new(move || {
            if let Some(w) = w.borrow().as_ref() {
                (w.draw)(&state.borrow());
            }
        })
    };
    let widget = (m.make)(&crate::cc::Ctx { close, flip: flip.clone(), again, state: state.clone() });
    let own = widget.face.as_ref().map(|f| f.click.clone()).unwrap_or_default();
    match &widget.face {
        Some(f) => face.append(&f.root),
        None => face.append(&gtk4::Image::from_icon_name(m.icon)),
    }
    face.set_tooltip_text(Some(m.name));
    // its menu, if it has one, in a popup of its own
    if let Some(card) = &widget.menu {
        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        body.add_css_class("surface");
        body.append(card);
        card.set_visible(true);
        *popup.borrow_mut() = Some(Popup::new(&cx.host, &s, side, 390, &body));
    }
    let click = gtk4::GestureClick::new();
    click.set_button(0);
    click.connect_released(move |g, _, _, _| {
        let mine = own.borrow().clone();
        match mine {
            Some(f) if g.current_button() == gtk4::gdk::BUTTON_PRIMARY => f(),
            _ => flip(),
        }
    });
    face.add_controller(click);
    face.set_cursor_from_name(Some("pointer"));
    *w.borrow_mut() = Some(widget);
    let w2 = w.clone();
    cx.hub.on(move |st| {
        *state.borrow_mut() = st.clone();
        if let Some(w) = w2.borrow().as_ref() {
            (w.draw)(st);
            if let Some(f) = &w.face {
                f.root.set_visible(true);
            }
        }
    });
    let p = popup.borrow().clone();
    Some(Block { popup: p, ..Block::new(&s) })
}
