//! The look picked by the dotfiles' theme, and the wallpaper: its state, its toggle with the wallpapers.

mod service;
mod widget;

use super::{widget, Fut, Module, TOGGLE};
use crate::services::Ctx;

pub const MODULE: Module = Module {
    id: "theme",
    state: Some(state),
    widgets: &[widget("wallpaper", "Wallpaper", "preferences-desktop-wallpaper-symbolic", TOGGLE, widget::wallpaper)
        .settings(widget::wallpaper_settings)],
    ..Module::NONE
};

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::state() })
}
