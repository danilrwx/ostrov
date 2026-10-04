//! The night light, through Hyprland's screen shader: its state, `ostrov night ...`, kept to its schedule.

pub mod service;

use std::time::Duration;

use super::{words, Fut, Module};
use crate::services::{every, Ctx, Kick, Res};

pub const MODULE: Module = Module {
    id: "night",
    forms: &["mode off|on|time|sun", "time FROM TO", "temp K", "preview K", "apply"],
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    ..Module::NONE
};

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::state() })
}

fn run(_: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(&words(&args)).await })
}

fn worker(_: std::sync::Arc<Ctx>, _: Kick) -> Fut<'static, ()> {
    Box::pin(every(Duration::from_secs(3), || async {
        if let Err(e) = service::apply_now().await {
            eprintln!("ostrov: night: {e}");
        }
    }))
}
