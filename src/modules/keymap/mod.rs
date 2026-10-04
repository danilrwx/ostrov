//! The keyboard's layout, from the compositor: its state, kicked on every switch.

mod service;

use super::{Fut, Module};
use crate::services::{Ctx, Kick};

pub const MODULE: Module = Module { id: "keymap", state: Some(state), worker: Some(worker), ..Module::NONE };

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::keymap().await.into() })
}

fn worker(_: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(service::events(kick))
}
