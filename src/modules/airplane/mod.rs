//! Airplane Mode, through rfkill: every radio soft-blocked, `ostrov airplane on|off|toggle`, its toggle.

mod service;
mod widget;

use super::{widget, words, Fut, Module};
use crate::cc::Show;
use crate::services::{Ctx, Res};

pub const MODULE: Module = Module {
    id: "airplane",
    forms: &["on|off|toggle"],
    state: Some(state),
    run: Some(run),
    widgets: &[widget("airplane", "Airplane Mode", "airplane-mode-symbolic", &[(2, 1), (4, 1)], widget::airplane)
        .bar(Show::Active)],
    ..Module::NONE
};

// no worker: the radios are read on the watcher's 3 s tick, rfkill telling nothing over a bus
fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::state() })
}

fn run(_: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(&words(&args)) })
}
