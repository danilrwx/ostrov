//! Motion: a value carried from one end to the other over a time, along a curve, frame by frame on the widget's
//! frame clock. Off ([appearance] animations = false, GTK's own setting) it lands at once.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

/// How a motion goes from 0 to 1 over its time.
#[derive(Clone, Copy)]
pub enum Curve {
    /// fast, then settling: what opens and moves
    Out,
    /// slow, then fast: what goes away
    In,
}

impl Curve {
    pub fn at(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Curve::Out => 1.0 - (1.0 - t).powi(3),
            Curve::In => t * t * t,
        }
    }
}

/// Whether things move: GTK's setting, which [appearance] animations sets.
pub fn on() -> bool {
    gtk4::Settings::default().is_none_or(|s| s.is_gtk_enable_animations())
}

/// A motion running on a widget: dropped, or another started on it, it stops where it is.
#[derive(Default)]
pub struct Motion(RefCell<Option<gtk4::TickCallbackId>>);

impl Motion {
    /// From 0 to 1 over ms along the curve, step(v) every frame (and step(1) at the end, or at once when nothing
    /// moves); a motion still running stopped first.
    pub fn run(self: &Rc<Self>, w: &impl IsA<gtk4::Widget>, ms: u32, curve: Curve, step: impl Fn(f64) + 'static) {
        self.stop();
        if !on() || ms == 0 || !w.is_mapped() {
            return step(1.0);
        }
        let start = RefCell::new(None::<i64>);
        let me = Rc::downgrade(self);
        let id = w.add_tick_callback(move |_, clock| {
            let now = clock.frame_time();
            let t0 = *start.borrow_mut().get_or_insert(now);
            let t = (now - t0) as f64 / (ms as f64 * 1000.0);
            step(curve.at(t));
            if t >= 1.0 {
                if let Some(m) = me.upgrade() {
                    m.0.borrow_mut().take();
                }
                return gtk4::glib::ControlFlow::Break;
            }
            gtk4::glib::ControlFlow::Continue
        });
        *self.0.borrow_mut() = Some(id);
    }

    pub fn stop(&self) {
        if let Some(id) = self.0.borrow_mut().take() {
            id.remove();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Curve;

    #[test]
    fn curves_end_where_they_should() {
        for c in [Curve::Out, Curve::In] {
            assert!(c.at(0.0).abs() < 1e-9);
            assert!((c.at(1.0) - 1.0).abs() < 1e-9);
            assert!(c.at(0.5) > 0.0 && c.at(0.5) < 1.0);
        }
    }
}
