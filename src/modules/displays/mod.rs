//! The monitors, through Hyprland: their state, `ostrov displays ...`, the profiles; its toggle with them.

mod service;
mod widget;

use super::{widget, words, Fut, Module, TOGGLE};
use crate::services::{Ctx, Kick, Res};

pub const USAGE: &str = "displays set NAME MODE POSITION SCALE|on NAME|off NAME|mirror NAME OF|save NAME|load NAME|delete NAME";

pub const MODULE: Module = Module {
    id: "displays",
    usage: USAGE,
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[widget("displays", "Displays", "video-display-symbolic", TOGGLE, widget::displays)],
};

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::state() })
}

fn run(_: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(&words(&args)).await })
}

fn worker(_: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(service::events(kick))
}
