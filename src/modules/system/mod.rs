//! The session's buttons: a screenshot, the lock, the power menu; Keep Awake; the notifications.

mod widget;

use super::{widget, Module, BUTTON, TOGGLE};
use crate::cc::Show;

pub const MODULE: Module = Module {
    id: "system",
    widgets: &[
        widget("screenshot", "Screenshot", "applets-screenshooter-symbolic", BUTTON, widget::screenshot),
        widget("lock", "Lock", "system-lock-screen-symbolic", BUTTON, widget::lock),
        widget("session", "Power Off", "system-shutdown-symbolic", BUTTON, widget::session),
        widget("awake", "Keep Awake", "weather-clear-symbolic", TOGGLE, widget::awake).bar(Show::Active),
        widget("notifications", "Notifications", "preferences-system-notifications-symbolic", &[(4, 6), (4, 3), (4, 4), (4, 5), (4, 7), (4, 8), (8, 3), (8, 4), (8, 6)], widget::notifications)
            .bar(Show::Active),
    ],
    ..Module::NONE
};
