//! Bluetooth, through BlueZ: its state, `ostrov bt ...`, its toggle with the devices.

mod service;
mod widget;

use super::{widget, words, Fut, Module, TOGGLE};
use crate::services::{signals, Ctx, Kick, Res};

pub const USAGE: &str = "bt on|off|scan|connect ADDR|disconnect ADDR|pair ADDR|forget ADDR";

pub const MODULE: Module = Module {
    id: "bt",
    usage: USAGE,
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[widget("bt", "Bluetooth", "bluetooth-active-symbolic", TOGGLE, widget::bluetooth)],
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
