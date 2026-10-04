//! The session's buttons: a screenshot, the lock, the power menu; Keep Awake.

mod widget;

use super::{widget, Module, BUTTON, TOGGLE};

pub const MODULE: Module = Module {
    id: "system",
    widgets: &[
        widget("screenshot", "Screenshot", "applets-screenshooter-symbolic", BUTTON, widget::screenshot),
        widget("lock", "Lock", "system-lock-screen-symbolic", BUTTON, widget::lock),
        widget("session", "Power Off", "system-shutdown-symbolic", BUTTON, widget::session),
        widget("awake", "Keep Awake", "weather-clear-symbolic", TOGGLE, widget::awake),
    ],
    ..Module::NONE
};
