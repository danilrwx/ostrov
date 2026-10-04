//! ostrov's events: what happens on the desktop, a name and a JSON payload, for what extends it to follow:
//! scripts through D-Bus's Event signal (api.rs), KDL widgets through an `event` source, plugins through
//! on-shell-event. Some come from where they happen (the window focused, a workspace, the lock, the wallpaper);
//! the rest from the services' state as it changes (watch): the network joined, the power plugged or
//! not, a Bluetooth device, the default output, the player.
//!
//!     window      {"class", "title"}          the window focused ("" none)
//!     workspace   {}                          a workspace made, gone, focused
//!     lock        {}  unlock {}               the screen locked, unlocked
//!     wallpaper   {"on", "path"}              the wallpaper picked
//!     network     {"on", "ssid"}              Wi-Fi on or off, a network joined or left
//!     power       {"state", "plugged", "percent"}  the battery charging, discharging, full
//!     bluetooth   {"on", "connected"}         Bluetooth on or off, a device connected or not
//!     output      {"name"}                    the default sound output
//!     media       {"playing", "title", "artist"}  the player playing or not, a new track

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::{json, Value};

use crate::hub::{s, Hub};

thread_local! {
    static SUBS: RefCell<Vec<Box<dyn Fn(&str, &Value)>>> = RefCell::default();
}

/// f on every event from now on.
pub fn on(f: impl Fn(&str, &Value) + 'static) {
    SUBS.with(|s| s.borrow_mut().push(Box::new(f)));
}

/// An event to everyone who follows them.
pub fn emit(name: &str, payload: Value) {
    // taken out while they run: a follower may follow or emit itself
    let subs = SUBS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    for f in &subs {
        f(name, &payload);
    }
    SUBS.with(|s| {
        let mut now = s.borrow_mut();
        let added = std::mem::take(&mut *now);
        *now = subs;
        now.extend(added);
    });
}

/// The events the state tells of, each from its part as it was and as it is: a name, its payload, or None when
/// it says nothing new.
fn changes(before: &Value, now: &Value) -> Vec<(&'static str, Value)> {
    let mut out = Vec::new();
    let mut part = |name: &'static str, of: &dyn Fn(&Value) -> Value| {
        let (a, b) = (of(before), of(now));
        if a != b && !now.is_null() {
            out.push((name, b));
        }
    };
    part("network", &|st| json!({"on": st["wifi"]["on"].as_bool().unwrap_or(false), "ssid": s(&st["wifi"], &["ssid"])}));
    part("power", &|st| json!({"state": s(&st["battery"], &["state"]), "plugged": st["battery"]["state"] != "discharging"}));
    part("bluetooth", &|st| json!({"on": st["bt"]["on"].as_bool().unwrap_or(false), "connected": s(&st["bt"], &["connected"])}));
    part("output", &|st| {
        let def = st["audio"]["sinks"].as_array().into_iter().flatten().find(|d| d["def"] == true);
        json!({"name": def.map_or("", |d| s(d, &["name"]))})
    });
    part("media", &|st| {
        let m = &st["media"];
        json!({"playing": m["playing"].as_bool().unwrap_or(false), "title": s(m, &["title"]), "artist": s(m, &["artist"])})
    });
    // the battery's percent rides along with its state, not an event of its own every percent
    for (name, payload) in out.iter_mut() {
        if *name == "power" {
            payload["percent"] = now["battery"]["percent"].clone();
        }
    }
    out
}

/// The state's events from the hub's every state, the first one only setting what was.
pub fn watch(hub: &Rc<Hub>) {
    let last = RefCell::new(Value::Null);
    hub.on(move |st| {
        if st.is_null() {
            return;
        }
        let before = last.replace(st.clone());
        if before.is_null() {
            return;
        }
        for (name, payload) in changes(&before, st) {
            emit(name, payload);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_tells_only_what_changed() {
        let a = json!({"wifi": {"on": true, "ssid": "home"}, "battery": {"state": "discharging", "percent": 50},
            "media": {"playing": false}});
        let mut b = a.clone();
        b["battery"]["percent"] = json!(49);
        assert!(changes(&a, &b).is_empty());
        b["wifi"]["ssid"] = json!("work");
        b["battery"]["state"] = json!("charging");
        let names: Vec<&str> = changes(&a, &b).iter().map(|c| c.0).collect();
        assert_eq!(names, ["network", "power"]);
        assert_eq!(changes(&a, &b)[1].1["percent"], json!(49));
    }

    #[test]
    fn a_follower_may_follow_more() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s2 = seen.clone();
        on(move |n, _| {
            s2.borrow_mut().push(n.to_string());
            let s3 = s2.clone();
            on(move |n, _| s3.borrow_mut().push(format!("late {n}")));
        });
        emit("one", Value::Null);
        emit("two", Value::Null);
        assert_eq!(*seen.borrow(), ["one", "two", "late two"]);
    }
}
