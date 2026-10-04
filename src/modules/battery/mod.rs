//! The battery, through UPower: its state and its widget; where it stops charging (limit.rs), `ostrov battery
//! limit N` (`limit install` the udev rule letting it, as root), and its toggle.

pub mod limit;
pub mod service;
mod widget;

use super::{widget, words, Fut, Module, TOGGLE};
use crate::cc::Show;
use crate::services::{signals, Ctx, Kick, Res};

pub const MODULE: Module = Module {
    id: "battery",
    forms: &["limit PERCENT|install"],
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[
        widget("battery", "Battery", "battery-good-symbolic", &[(5, 1), (2, 1), (3, 1), (4, 1), (8, 1)], widget::battery)
            .bar(Show::Active),
        widget("charge", "Charge Limit", "battery-level-80-symbolic", TOGGLE, widget::charge),
    ],
    ..Module::NONE
};

/// UPower's battery, its thresholds under "limit".
fn state(c: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async move {
        let mut st = service::state(c).await;
        if let Some(o) = st.as_object_mut() {
            o.insert("limit".into(), limit::state());
        }
        st
    })
}

fn run(_: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move {
        match words(&args)[..] {
            ["limit", "install"] => limit::install().await,
            ["limit", n] => limit::set(n.parse().map_err(|_| MODULE.usage())?),
            _ => Err(MODULE.usage()),
        }
    })
}

fn worker(c: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(signals(c, "org.freedesktop.UPower", kick))
}
