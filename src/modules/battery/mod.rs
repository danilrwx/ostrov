//! The battery, through UPower: its state and its widget.

pub mod service;
mod widget;

use super::{widget, Fut, Module};
use crate::cc::Show;
use crate::services::{signals, Ctx, Kick};

pub const MODULE: Module = Module {
    id: "battery",
    state: Some(state),
    worker: Some(worker),
    widgets: &[widget("battery", "Battery", "battery-good-symbolic", &[(5, 1), (2, 1), (3, 1), (4, 1), (8, 1)], widget::battery).bar(Show::Active)],
    ..Module::NONE
};

fn state(c: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(service::state(c))
}

fn worker(c: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(signals(c, "org.freedesktop.UPower", kick))
}
