//! The workspaces as GNOME's dots: a dot each, the focused one a longer pill, a click to go there. A dot is made
//! once and kept, so the focused one's growing and the last one's shrinking can be drawn: every frame a step of
//! each dot's width towards its own (28 focused, 8 not), about 100 ms in all, the frame clock
//! let go once there (a CSS transition misses a dot made focused). Its monitor's workspaces, the one shown there
//! focused.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::glib;

use super::{Block, Ctx};
use crate::wm;

pub fn build(cx: &Rc<Ctx>) -> Block {
    let dots = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    dots.set_valign(gtk4::Align::Center);

    let growing = Rc::new(Cell::new(false));
    let grow = {
        let (dots, growing) = (dots.clone(), growing.clone());
        move || {
            if growing.replace(true) {
                return;
            }
            let last = Cell::new(0i64);
            let growing = growing.clone();
            dots.add_tick_callback(move |dots, clock| {
                let now = clock.frame_time();
                let dt = if last.get() == 0 { 16_000 } else { now - last.get() };
                last.set(now);
                let k = 1.0 - (-(dt as f64) / 30_000.0).exp();
                let mut moving = false;
                let mut c = dots.first_child();
                while let Some(dot) = c {
                    let to = if dot.has_css_class("focused") { 28.0 } else { 8.0 };
                    let w = dot.width_request().max(8) as f64;
                    let step = (to - w) * k;
                    let next = if (to - w).abs() < 1.0 { to } else { w + if step.abs() < 1.0 { step.signum() } else { step } };
                    dot.set_size_request(next.round() as i32, 8);
                    moving |= next != to;
                    c = dot.next_sibling();
                }
                if moving {
                    glib::ControlFlow::Continue
                } else {
                    growing.set(false);
                    glib::ControlFlow::Break
                }
            });
        }
    };
    let draw = {
        let (dots, host) = (dots.clone(), cx.host.clone());
        let made: Rc<RefCell<Vec<(i64, gtk4::Box)>>> = Rc::default();
        move || {
            let (ids, active) = wm::workspaces(host.connector().as_deref());
            let mut made = made.borrow_mut();
            // gone workspaces out, new ones in at their place
            made.retain(|(id, dot)| {
                let keep = ids.contains(id);
                if !keep {
                    dots.remove(dot);
                }
                keep
            });
            for (i, id) in ids.iter().enumerate() {
                if made.iter().any(|(m, _)| m == id) {
                    continue;
                }
                let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                dot.add_css_class("dot");
                dot.set_valign(gtk4::Align::Center);
                let id = *id;
                let click = gtk4::GestureClick::new();
                click.connect_released(move |_, _, _, _| wm::go(id));
                dot.add_controller(click);
                dot.set_cursor_from_name(Some("pointer"));
                let after = if i == 0 { None } else { made.iter().find(|(m, _)| *m == ids[i - 1]).map(|(_, d)| d.clone()) };
                dots.insert_child_after(&dot, after.as_ref());
                let at = i.min(made.len());
                made.insert(at, (id, dot));
            }
            for (id, dot) in made.iter() {
                if Some(*id) == active {
                    dot.add_css_class("focused");
                } else {
                    dot.remove_css_class("focused");
                }
            }
            grow();
        }
    };
    draw();
    cx.on_wm(move |e| {
        if matches!(e, wm::Event::Workspaces) {
            draw();
        }
    });
    Block::new(&dots)
}
