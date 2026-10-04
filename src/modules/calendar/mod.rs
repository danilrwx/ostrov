//! The calendar's events, its sources' (plugins'): their state, `ostrov calendar refresh`; the month and the
//! coming events.

pub mod service;
mod widget;

use super::{widget, words, Fut, Module};
use crate::cc::Show;
use crate::services::{Ctx, Kick, Res};

pub const MODULE: Module = Module {
    id: "calendar",
    forms: &["refresh"],
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[
        widget("month", "Month", "x-office-calendar-symbolic", &[(4, 5), (4, 6), (4, 7), (8, 5), (8, 6), (8, 7)], widget::month),
        widget("agenda", "Coming Up", "view-list-symbolic", &[(4, 4), (4, 2), (4, 3), (4, 5), (4, 6), (8, 2), (8, 3), (8, 4)], widget::agenda).bar(Show::Active),
    ],
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
