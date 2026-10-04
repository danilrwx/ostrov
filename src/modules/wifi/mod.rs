//! Wi-Fi, through iwd or NetworkManager: its state, `ostrov wifi ...`, its toggle with the networks.

mod nm;
mod service;
mod widget;

use serde_json::Value;
use crate::cc::Show;

use super::{values, widget, words, Fut, Module, TOGGLE};
use crate::services::{signals, Ctx, Kick, Res};

pub const MODULE: Module = Module {
    id: "wifi",
    forms: &["on|off|scan|disconnect", "connect SSID", "forget SSID"],
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[widget("wifi", "Wi-Fi", "network-wireless-symbolic", TOGGLE, widget::wifi).bar(Show::Always)],
    complete: Some(complete),
};

/// SSID: the networks in sight, their security, bars, whether known and connected.
fn complete(st: &Value, _: &str) -> Vec<(String, String)> {
    values(&st["networks"], "ssid", |n| {
        let mut about = format!("{}, {}/4", n["security"].as_str().unwrap_or(""), n["signal"]);
        for flag in ["known", "connected"].into_iter().filter(|f| n[*f] == true) {
            about += &format!(", {flag}");
        }
        about
    })
}

fn state(c: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(service::state(c))
}

fn run(c: &Ctx, args: Vec<String>, input: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(c, &words(&args), input).await })
}

fn worker(c: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(async move {
        tokio::join!(signals(c.clone(), service::IWD, kick.clone()), signals(c, nm::NM, kick));
    })
}

