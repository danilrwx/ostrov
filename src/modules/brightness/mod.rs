//! The screen's brightness, through logind: its state, `ostrov brightness PERCENT`, its slider (the night light
//! behind its arrow).

pub mod service;
mod widget;

use super::{widget, words, Fut, Module, SLIDER};
use crate::services::{Ctx, Res};

pub const MODULE: Module = Module {
    id: "brightness",
    forms: &["PERCENT"],
    state: Some(state),
    run: Some(run),
    widgets: &[widget("brightness", "Brightness", "display-brightness-symbolic", SLIDER, widget::brightness)],
    ..Module::NONE
};

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::brightness().into() })
}

fn run(c: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(c, &words(&args)).await })
}
