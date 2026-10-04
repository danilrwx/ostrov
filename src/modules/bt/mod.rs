//! Bluetooth, through BlueZ: its state, `ostrov bt ...`, its toggle with the devices.

mod service;
mod widget;

use super::{values, widget, words, Fut, Module, TOGGLE};
use crate::cc::Show;
use crate::services::{signals, Ctx, Kick, Res};

const BATTERIES: &[(u8, u8)] = &[(4, 1), (4, 2), (8, 1), (8, 2)];

pub const MODULE: Module = Module {
    id: "bt",
    forms: &["on|off|scan", "connect|disconnect|pair|forget ADDR"],
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[
        widget("bt", "Bluetooth", "bluetooth-active-symbolic", TOGGLE, widget::bluetooth),
        widget("bt-battery", "Device Batteries", "battery-good-symbolic", BATTERIES, widget::batteries)
            .bar(Show::Active),
    ],
    // ADDR: the devices paired or found, by name
    complete: Some(|st, _| values(&st["devices"], "address", |d| d["name"].as_str().unwrap_or("").into())),
};

fn state(c: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(service::state(c))
}

fn run(c: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(c, &words(&args)).await })
}

fn worker(c: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(signals(c, "org.bluez", kick))
}
