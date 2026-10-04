//! The weather now, from open-meteo, every quarter of an hour; its widget, its badge the bar's weather.

mod service;
mod widget;

use super::{widget, Fut, Module};
use crate::cc::Show;
use crate::services::{Ctx, Kick};

pub const MODULE: Module = Module {
    id: "weather",
    state: Some(state),
    worker: Some(worker),
    widgets: &[widget("weather", "Weather", "weather-few-clouds-symbolic", &[(4, 2), (8, 2), (4, 1), (2, 1)], widget::weather).bar(Show::Always)],
    ..Module::NONE
};

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::state() })
}

fn worker(_: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(service::run(kick))
}
