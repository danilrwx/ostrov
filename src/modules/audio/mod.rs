//! Sound, through PipeWire: its state (devices, the apps playing, the mic and camera in use), `ostrov audio
//! ...`, `ostrov headset`; the volume and mic sliders, the headset's toggle.

pub mod service;
mod widget;

use std::time::Duration;

use super::{widget, words, Fut, Module, SLIDER, TOGGLE};
use crate::services::{every, Ctx, Kick, Res};

pub const MODULE: Module = Module {
    id: "audio",
    forms: &["volume ID LEVEL"],
    // ID: the streams playing, by their app
    complete: Some(|st, _| super::values(&st["streams"], "id", |s| s["name"].as_str().unwrap_or("").into())),
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[
        widget("volume", "Volume", "audio-volume-high-symbolic", SLIDER, widget::volume),
        widget("mic", "Microphone", "microphone-sensitivity-high-symbolic", SLIDER, widget::mic),
        widget("headset", "Headset", "audio-headphones-symbolic", TOGGLE, widget::headset),
    ],
};

/// The headset's mode flipped, handsfree (with its mic) or headphones.
pub const HEADSET: Module = Module { id: "headset", forms: &[""], run: Some(headset), ..Module::NONE };

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(service::state())
}

fn run(_: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(&words(&args)).await })
}

fn headset(_: &Ctx, _: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(service::headset())
}

/// PipeWire's events, and the cameras looked over every 3 s (nothing tells when one opens).
fn worker(_: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(async move {
        tokio::spawn(every(Duration::from_secs(3), service::scan_cameras));
        service::events(kick).await
    })
}
