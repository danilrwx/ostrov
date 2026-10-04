//! The weather now, from open-meteo, every quarter of an hour.

mod service;

use super::{Fut, Module};
use crate::services::{Ctx, Kick};

pub const MODULE: Module = Module { id: "weather", state: Some(state), worker: Some(worker), ..Module::NONE };

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::state() })
}

fn worker(_: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(service::run(kick))
}
