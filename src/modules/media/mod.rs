//! The player, through MPRIS: its state, `ostrov media ...`; the Now Playing widget.

mod service;
mod widget;

use super::{widget, words, Fut, Module};
use crate::services::{Ctx, Kick, Res};

pub const MODULE: Module = Module {
    id: "media",
    forms: &["play-pause|next|previous"],
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[widget("media", "Now Playing", "audio-x-generic-symbolic", &[(4, 2), (8, 2)], widget::player)],
    ..Module::NONE
};

fn state(c: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(service::state(c))
}

fn run(c: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(c, &words(&args)).await })
}

fn worker(c: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(service::events(c, kick))
}
