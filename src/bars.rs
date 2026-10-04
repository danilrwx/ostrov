//! A bar on every monitor ([bar] monitors: all of them, the primary alone, the ones named), following monitors
//! plugged and unplugged. The first bar, the primary monitor's, is the launcher's and is never gone: its monitor
//! unplugged, it moves to the next; the bars past the monitors left are. What is one by nature stays one: the tray
//! (bar/tray.rs), a panel (bar/panel.rs), the compositor's events (bar/mod.rs).

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;

use crate::bar::Bar;
use crate::config::{Monitors, Which};
use crate::hub::Hub;
use crate::popup::Host;

pub struct Bars {
    app: gtk4::Application,
    hub: Rc<Hub>,
    layout: [Vec<String>; 3],
    monitors: Monitors,
    /// the monitor focused as ostrov starts: "primary"'s, the first bar's
    primary: Option<String>,
    /// each bar's window, the bar, the overlay over its strip (the launcher's in the first)
    pub all: RefCell<Vec<(Rc<Host>, Rc<Bar>, gtk4::Overlay)>>,
}

impl Bars {
    /// The bars on the monitors as they are, following them as they change.
    pub fn start(app: &gtk4::Application, hub: &Rc<Hub>, cfg: crate::config::Bar) -> Rc<Bars> {
        let bars = Rc::new(Bars {
            app: app.clone(),
            hub: hub.clone(),
            layout: [cfg.left, cfg.center, cfg.right],
            monitors: cfg.monitors,
            primary: crate::wm::focused_monitor(),
            all: RefCell::default(),
        });
        bars.update();
        if let Some(d) = gdk::Display::default() {
            let b = Rc::downgrade(&bars);
            d.monitors().connect_items_changed(move |_, _, _, _| {
                if let Some(b) = b.upgrade() {
                    b.update();
                }
            });
        }
        bars
    }

    /// A bar on each monitor chosen: the ones there moved to theirs, new ones made, the ones past them gone; the
    /// first always there.
    fn update(&self) {
        let ms: Vec<gdk::Monitor> = gdk::Display::default()
            .map(|d| d.monitors().iter::<gdk::Monitor>().filter_map(Result::ok).collect())
            .unwrap_or_default();
        let names: Vec<String> = ms.iter().map(|m| m.connector().map(|c| c.to_string()).unwrap_or_default()).collect();
        let want = chosen(&self.monitors, &names, self.primary.as_deref());
        let mut all = self.all.borrow_mut();
        // a new bar in the mode the others are in (ostrov bar toggle)
        let docked = all.first().is_none_or(|(h, ..)| h.docked.get());
        for (i, &k) in want.iter().enumerate() {
            match all.get(i) {
                Some((host, bar, _)) => {
                    if host.set_monitor(&ms[k]) {
                        bar.moved();
                    }
                }
                None => all.push(self.build(Some(&ms[k]), docked)),
            }
        }
        if all.is_empty() {
            all.push(self.build(None, docked));
        }
        for (host, ..) in all.drain(want.len().max(1)..) {
            host.destroy();
        }
    }

    fn build(&self, monitor: Option<&gdk::Monitor>, docked: bool) -> (Rc<Host>, Rc<Bar>, gtk4::Overlay) {
        let over = gtk4::Overlay::new();
        let host = Host::new(&self.app, &over, monitor);
        fn names(v: &[String]) -> Vec<&str> {
            v.iter().map(String::as_str).collect()
        }
        let [l, c, r] = &self.layout;
        let (l, c, r) = (names(l), names(c), names(r));
        let bar = Bar::build(&host, &self.hub, [&l, &c, &r]);
        over.set_child(Some(&bar.strip));
        host.docked.set(docked);
        host.apply();
        (host, bar, over)
    }

    /// The first bar: the launcher's.
    pub fn first(&self) -> (Rc<Host>, Rc<Bar>, gtk4::Overlay) {
        self.all.borrow()[0].clone()
    }

    /// The focused monitor's bar, the first without a compositor that says.
    pub fn focused(&self) -> Rc<Bar> {
        let f = crate::popup::focused();
        let all = self.all.borrow();
        all.iter().find(|(h, ..)| f.as_ref().is_some_and(|f| Rc::ptr_eq(f, h))).unwrap_or(&all[0]).1.clone()
    }
}

/// The monitors with a bar, by their place in GDK's list of their names: all of them, the primary first; the primary
/// alone; the ones named, in that order. The primary is the one focused as ostrov starts, else the first; none of
/// those named there, the first: never no bar while there is a monitor.
fn chosen(which: &Monitors, names: &[String], primary: Option<&str>) -> Vec<usize> {
    if names.is_empty() {
        return vec![];
    }
    let p = primary.and_then(|p| names.iter().position(|n| n == p)).unwrap_or(0);
    let v: Vec<usize> = match which {
        Monitors::Which(Which::All) => std::iter::once(p).chain((0..names.len()).filter(|&i| i != p)).collect(),
        Monitors::Which(Which::Primary) => vec![p],
        Monitors::Named(list) => list.iter().filter_map(|n| names.iter().position(|m| m == n)).collect(),
    };
    if v.is_empty() { vec![0] } else { v }
}

#[cfg(test)]
mod tests {
    use super::chosen;
    use crate::config::{Monitors, Which};

    #[test]
    fn the_monitors_chosen() {
        let names: Vec<String> = ["eDP-1", "DP-2", "HDMI-A-1"].iter().map(|s| s.to_string()).collect();
        let all = Monitors::Which(Which::All);
        let primary = Monitors::Which(Which::Primary);
        assert_eq!(chosen(&all, &names, None), [0, 1, 2]);
        assert_eq!(chosen(&all, &names, Some("DP-2")), [1, 0, 2]);
        assert_eq!(chosen(&primary, &names, Some("HDMI-A-1")), [2]);
        // the primary unplugged since: the first
        assert_eq!(chosen(&primary, &names, Some("DP-3")), [0]);
        let named = |v: &[&str]| Monitors::Named(v.iter().map(|s| s.to_string()).collect());
        assert_eq!(chosen(&named(&["HDMI-A-1", "eDP-1"]), &names, Some("DP-2")), [2, 0]);
        assert_eq!(chosen(&named(&["DP-3", "DP-2"]), &names, None), [1]);
        assert_eq!(chosen(&named(&["DP-3"]), &names, None), [0]);
        assert_eq!(chosen(&all, &[], Some("eDP-1")), Vec::<usize>::new());
        assert_eq!(chosen(&all, &names[..1], None), [0]);
    }
}
