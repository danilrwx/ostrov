//! The wallpaper: ostrov's pick of it (service.rs), `ostrov wallpaper ...`, its toggle with the pictures. The
//! picture itself is drawn under everything by wallpaper.rs.

pub mod service;
mod widget;

use super::{widget, words, Fut, Module, TOGGLE};
use crate::services::{Ctx, Res};

pub const MODULE: Module = Module {
    id: "wallpaper",
    forms: &["on|off|random", "set PATH"],
    state: Some(state),
    run: Some(run),
    widgets: &[widget("wallpaper", "Wallpaper", "preferences-desktop-wallpaper-symbolic", TOGGLE, widget::wallpaper)
        .settings(widget::wallpaper_settings)],
    ..Module::NONE
};

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::state() })
}

fn run(_: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(&words(&args)).await })
}
