//! The look picked by the dotfiles' theme, and the wallpaper: its state, its toggle with the wallpapers.

mod service;
mod widget;

use super::{Fut, Module, WidgetDef, TOGGLE};
use crate::services::Ctx;

pub const MODULE: Module = Module {
    id: "theme",
    state: Some(state),
    widgets: &[WidgetDef {
        id: "wallpaper",
        name: "Wallpaper",
        icon: "preferences-desktop-wallpaper-symbolic",
        sizes: TOGGLE,
        make: widget::wallpaper,
        settings: Some(widget::wallpaper_settings),
    }],
    ..Module::NONE
};

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(async { service::state() })
}
