//! The power profile, through power-profiles-daemon: its state, `ostrov power set PROFILE`, its toggle.

pub mod service;
mod widget;

use super::{widget, words, Fut, Module, TOGGLE};
use crate::services::{signals, Ctx, Kick, Res};

pub const USAGE: &str = "power set PROFILE";

pub const MODULE: Module = Module {
    id: "power",
    usage: USAGE,
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[widget("power", "Power Mode", "power-profile-balanced-symbolic", TOGGLE, widget::power)],
};

fn state(c: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(service::state(c))
}

fn run(c: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(c, &words(&args)).await })
}

fn worker(c: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(signals(c, "net.hadess.PowerProfiles", kick))
}
