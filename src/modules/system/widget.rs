
use gtk4::prelude::*;

use crate::cc::{Ctx, Face, Widget};
use crate::style::label;
use crate::hub::run;
use crate::i18n::t;
use crate::ui::{menu, round, row, Toggle};

pub fn screenshot(c: &Ctx) -> Widget {
    let close = c.close.clone();
    let b = round("applets-screenshooter-symbolic", move || {
        close();
        // once the panel has rolled up out of the picture
        let me = std::env::current_exe().unwrap_or_default();
        run(&["sh", "-c", "sleep 0.2; exec \"$0\" screenshot", &me.to_string_lossy()]);
    });
    b.set_tooltip_text(Some(t("Screenshot")));
    Widget::new(&b, None, |_| ())
}

pub fn lock(c: &Ctx) -> Widget {
    let close = c.close.clone();
    let b = round("system-lock-screen-symbolic", move || {
        close();
        run(&["loginctl", "lock-session"]);
    });
    b.set_tooltip_text(Some(t("Lock")));
    Widget::new(&b, None, |_| ())
}

/// The power menu's button: suspend, restart, power off, log out under it.
pub fn session(c: &Ctx) -> Widget {
    let flip = c.flip.clone();
    let b = round("system-shutdown-symbolic", move || flip());
    b.set_tooltip_text(Some(t("Power Off")));
    let (card, items) = menu("system-shutdown-symbolic", t("Power Off"));
    for (icon, text, cmd) in [
        ("weather-clear-night-symbolic", "Suspend", vec!["systemctl", "suspend"]),
        ("view-refresh-symbolic", "Restart…", vec!["systemctl", "reboot"]),
        ("system-shutdown-symbolic", "Power Off…", vec!["systemctl", "poweroff"]),
        ("system-log-out-symbolic", "Log Out", vec!["hyprctl", "dispatch", "exit"]),
    ] {
        let close = c.close.clone();
        items.append(&row(icon, t(text), "", false, move || {
            close();
            run(&cmd);
        }));
    }
    Widget::new(&b, Some(&card), |_| ())
}

/// Keep Awake: idle neither locks nor turns the screens off while it is on (idle.rs).
pub fn awake(c: &Ctx) -> Widget {
    let again = c.again.clone();
    let tg = Toggle::new("weather-clear-symbolic", t("Keep Awake"), move || {
        crate::idle::set_awake(!crate::idle::awake());
        again();
    }, None);
    let t2 = tg.clone();
    Widget::toggle(&tg, None, move |_| {
        let on = crate::idle::awake();
        t2.set(on, "", if on { t("the screen stays on") } else { "" });
    })
}

/// The notifications' history, newest first (a click dismisses one, its actions as buttons), Do Not Disturb and
/// Clear under it; its badge a bell struck through while they keep quiet (Do Not Disturb, a game, the quiet
/// hours of [notifications]).
pub fn notifications(_: &Ctx) -> Widget {
    let col = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    col.add_css_class("card");
    col.append(&label(t("Notifications"), "title"));
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    scroll.set_child(Some(&list));
    col.append(&scroll);
    let foot = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let dnd = gtk4::ToggleButton::with_label(t("Do Not Disturb"));
    dnd.add_css_class("chip");
    dnd.set_hexpand(true);
    dnd.set_halign(gtk4::Align::Start);
    let clear_all = gtk4::Button::with_label(t("Clear"));
    clear_all.add_css_class("chip");
    foot.append(&dnd);
    foot.append(&clear_all);
    col.append(&foot);
    let (face, _) = Face::icon("notifications-disabled-symbolic");
    let active = face.active.clone();
    let Some(notes) = crate::notes::get() else { return Widget::new(&col, None, |_| ()) };

    let (list2, n2, dnd2) = (list.downgrade(), notes.clone(), dnd.downgrade());
    let draw = move || {
        let (Some(list), Some(dnd)) = (list2.upgrade(), dnd2.upgrade()) else { return };
        crate::style::clear(&list);
        let hist = n2.history();
        if hist.is_empty() {
            let l = label(t("No notifications"), "dim");
            l.set_xalign(0.5);
            l.set_margin_top(40);
            list.append(&l);
        }
        for n in hist.iter().rev() {
            list.append(&n2.card(n, true));
        }
        dnd.set_active(n2.dnd());
        // quiet by itself (a game focused, the quiet hours): its bell struck through all the same, the chip said so
        let quiet = n2.quiet_now();
        active.set(quiet);
        dnd.set_label(if quiet && !n2.dnd() { t("Quiet for now") } else { t("Do Not Disturb") });
    };
    draw();
    notes.on_change(draw);
    let n3 = notes.clone();
    dnd.connect_toggled(move |b| {
        if n3.dnd() != b.is_active() {
            n3.set_dnd(b.is_active());
        }
    });
    clear_all.connect_clicked(move |_| notes.clear());
    Widget { face: Some(face), ..Widget::new(&col, None, |_| ()) }
}
