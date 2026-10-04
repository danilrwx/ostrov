//! The monitors, through Hyprland: their state, `ostrov displays ...`, the profiles; its toggle with them.

mod service;
mod widget;

use serde_json::Value;

use super::{values, widget, words, Fut, Module, TOGGLE};
use crate::services::{Ctx, Kick, Res};

pub const MODULE: Module = Module {
    id: "displays",
    forms: &["set NAME MODE POSITION SCALE", "on|off NAME", "mirror NAME OF", "save|load|delete PROFILE"],
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[widget("displays", "Displays", "video-display-symbolic", TOGGLE, widget::displays)],
    complete: Some(complete),
};

/// NAME, OF: the monitors, by their description; PROFILE: the profiles saved.
fn complete(st: &Value, ph: &str) -> Vec<(String, String)> {
    match ph {
        "NAME" | "OF" => values(&st["monitors"], "name", |m| m["description"].as_str().unwrap_or("").into()),
        "PROFILE" => values(&st["profiles"], "", |_| String::new()),
        _ => Vec::new(),
    }
}

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::state() })
}

fn run(_: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(&words(&args)).await })
}

fn worker(_: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(service::events(kick))
}
