//! The window focused: its app's icon and its title, as the compositor says on every change; on the bar of the
//! monitor focused, the others keeping the one last focused on theirs.

use std::rc::Rc;

use gtk4::prelude::*;

use super::{pill, slot, Block, Ctx};
use crate::wm::Event;

pub fn build(cx: &Rc<Ctx>) -> Block {
    let p = pill();
    let icon = gtk4::Image::new();
    let title = gtk4::Label::new(None);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.set_max_width_chars(48);
    p.append(&icon);
    p.append(&title);
    p.set_visible(false);
    let s = slot(&p);
    let host = cx.host.clone();
    cx.on_wm(move |e| {
        let Event::Window(class, t) = e else { return };
        if !crate::popup::focused().is_some_and(|f| Rc::ptr_eq(&f, &host)) {
            return;
        }
        p.set_visible(!class.is_empty() || !t.is_empty());
        match app_icon(class) {
            Some(g) => icon.set_from_gicon(&g),
            None => icon.set_icon_name(Some("application-x-executable-symbolic")),
        }
        title.set_text(t);
        p.set_tooltip_text(Some(t));
    });
    Block::new(&s)
}

/// A window's app icon: its class's desktop file ("<class>.desktop", as is or lowercased), else the app whose
/// StartupWMClass is the class. The screen-share picker's windows' too (share.rs).
pub fn app_icon(class: &str) -> Option<gio::Icon> {
    let by_name = [class.to_string(), class.to_lowercase()]
        .into_iter()
        .find_map(|c| gio_unix::DesktopAppInfo::new(&format!("{c}.desktop")));
    let app = by_name.or_else(|| {
        gio::AppInfo::all().into_iter().filter_map(|a| a.downcast::<gio_unix::DesktopAppInfo>().ok()).find(|d| {
            d.startup_wm_class().is_some_and(|w| w.eq_ignore_ascii_case(class))
        })
    })?;
    app.icon()
}
