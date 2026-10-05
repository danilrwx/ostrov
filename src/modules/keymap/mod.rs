//! The keyboard's layout, from Hyprland: its layouts and the active one, kicked on every switch; `ostrov keymap
//! next|set N`; its widget, the layout as a flag, a language's two letters or its name, a click the next one, its
//! menu every layout. The bar's `layout` block is this widget.

pub mod service;
mod widget;

use super::{widget, words, Fut, Module};
use crate::cc::Show;
use crate::services::{Ctx, Kick, Res};

pub const MODULE: Module = Module {
    id: "keymap",
    forms: &["next", "set N"],
    state: Some(state),
    run: Some(run),
    worker: Some(worker),
    widgets: &[widget("keymap", "Keyboard Layout", "input-keyboard-symbolic", &[(2, 1), (1, 1), (4, 1), (8, 1)], widget::keymap)
        .bar(Show::Always)
        .settings(widget::settings)],
    ..Module::NONE
};

fn state(_: &Ctx) -> Fut<'_, serde_json::Value> {
    Box::pin(service::keymap())
}

fn run(_: &Ctx, args: Vec<String>, _: Option<String>) -> Fut<'_, Res> {
    Box::pin(async move { service::cmd(&words(&args)).await })
}

fn worker(_: std::sync::Arc<Ctx>, kick: Kick) -> Fut<'static, ()> {
    Box::pin(service::events(kick))
}
