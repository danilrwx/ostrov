
use gtk4::prelude::*;

use crate::cc::{Ctx, Widget};
use crate::hub::run;
use crate::ui::{menu, round, row, Toggle};

pub fn screenshot(c: &Ctx) -> Widget {
    let close = c.close.clone();
    let b = round("applets-screenshooter-symbolic", move || {
        close();
        // once the panel has rolled up out of the picture
        let me = std::env::current_exe().unwrap_or_default();
        run(&["sh", "-c", "sleep 0.2; exec \"$0\" screenshot", &me.to_string_lossy()]);
    });
    b.set_tooltip_text(Some("Screenshot"));
    Widget::new(&b, None, |_| ())
}

pub fn lock(c: &Ctx) -> Widget {
    let close = c.close.clone();
    let b = round("system-lock-screen-symbolic", move || {
        close();
        run(&["loginctl", "lock-session"]);
    });
    b.set_tooltip_text(Some("Lock"));
    Widget::new(&b, None, |_| ())
}

/// The power menu's button: suspend, restart, power off, log out under it.
pub fn session(c: &Ctx) -> Widget {
    let flip = c.flip.clone();
    let b = round("system-shutdown-symbolic", move || flip());
    b.set_tooltip_text(Some("Power Off"));
    let (card, items) = menu("system-shutdown-symbolic", "Power Off");
    for (icon, text, cmd) in [
        ("weather-clear-night-symbolic", "Suspend", vec!["systemctl", "suspend"]),
        ("view-refresh-symbolic", "Restart…", vec!["systemctl", "reboot"]),
        ("system-shutdown-symbolic", "Power Off…", vec!["systemctl", "poweroff"]),
        ("system-log-out-symbolic", "Log Out", vec!["sh", "-c", "hyprctl dispatch exit || swaymsg exit"]),
    ] {
        let close = c.close.clone();
        items.append(&row(icon, text, "", false, move || {
            close();
            run(&cmd);
        }));
    }
    Widget::new(&b, Some(&card), |_| ())
}

/// Keep Awake: idle neither locks nor turns the screens off while it is on (idle.rs).
pub fn awake(c: &Ctx) -> Widget {
    let again = c.again.clone();
    let t = Toggle::new("weather-clear-symbolic", "Keep Awake", move || {
        crate::idle::set_awake(!crate::idle::awake());
        again();
    }, None);
    let t2 = t.clone();
    Widget::toggle(&t, None, move |_| {
        let on = crate::idle::awake();
        t2.set(on, "", if on { "the screen stays on" } else { "" });
    })
}
