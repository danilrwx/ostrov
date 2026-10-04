//! Wi-Fi, through iwd: its state, `ostrov wifi ...`, its toggle with the networks.

mod service;
mod widget;

use super::{widget, words, Fut, Module, TOGGLE};
use crate::cc::Show;
use crate::services::{signals, Ctx, Kick, Res};

pub const USAGE: &str = "wifi on|off|scan|disconnect|connect SSID|forget SSID";

pub const MODULE: Module = Module {
    id: "wifi",
    usage: USAGE,
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[widget("wifi", "Wi-Fi", "network-wireless-symbolic", TOGGLE, widget::wifi).bar(Show::Always)],
};

fn state(c: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(service::state(c))
}

fn run(c: &Ctx, args: Vec<String>, input: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(c, &words(&args), input).await })
}

fn worker(c: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(signals(c, "net.connman.iwd", kick))
}

