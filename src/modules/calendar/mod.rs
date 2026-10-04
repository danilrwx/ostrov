//! The calendars' events, CalDAV's and .ics links': their state, `ostrov calendar refresh`.

pub mod service;

use super::{words, Fut, Module};
use crate::services::{Ctx, Kick, Res};

pub const USAGE: &str = "calendar refresh";

pub const MODULE: Module = Module {
    id: "calendar",
    usage: USAGE,
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

fn worker(_: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(service::run(kick))
}
