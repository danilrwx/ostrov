//! How ostrov looks, all in one place: the palette (every colour a name, set here alone), the shapes, the CSS
//! every part draws with, and the few widget helpers they share. The bar's black follows bin/theme's alpha
//! (~/.cache/theme/alpha, 1 under dark), reloaded as it changes.

use gtk4::prelude::*;
use gtk4::gio;

use crate::hub::home;

const CSS: &str = r#"
/* the palette */
@define-color fg #ffffff;                       /* text, icons */
@define-color dim #888888;                      /* what is said second: a subtitle, a time, a day of another month */
@define-color ink #000000;                      /* text and icons on the accent */
@define-color accent #ffffff;                   /* on, picked, today, a level */
@define-color accent-rule #999999;              /* a line on the accent */
@define-color accent-pressed #d0d0d0;           /* the accent pressed (a toggle's open side) */
@define-color bar rgba(0, 0, 0, ALPHA);         /* the bar's black over the wallpaper */
@define-color surface rgba(0, 0, 0, 0.75);      /* the ground of everything that opens: panels, the calendar, the
                                                   tray's menus, toasts, the OSD, a tab and a hovered block; the
                                                   bar's black a shade denser, past Hyprland's ignore_alpha 0.7 so
                                                   blurred where the bar is not */
@define-color hover rgba(255, 255, 255, 0.15);  /* under the pointer */
@define-color raised rgba(255, 255, 255, 0.08); /* a button, a toggle, a chip on a surface */
@define-color card rgba(255, 255, 255, 0.06);   /* a card, a menu, the month on a surface */
@define-color well rgba(255, 255, 255, 0.1);    /* a trough, a line inside a surface */
@define-color sunk rgba(0, 0, 0, 0.2);          /* an entry */
@define-color rule #333333;                     /* an outline */
@define-color idle #666666;                     /* a workspace not focused */
@define-color urgent #cd0000;                   /* an error, a critical notification */
@define-color lock #000000;                     /* the lock screen */

/* the shapes: a surface rounded 10, what is on it 6 */
* { font-family: "Iosevka"; font-size: 11pt; color: @fg; }
image { -gtk-icon-size: 16px; }
window { background: transparent; }
scrolledwindow { background: none; }
button { background: none; border: none; box-shadow: none; outline: none; min-height: 0; min-width: 0; padding: 0; }
separator { background: @rule; margin: 4px; min-height: 1px; min-width: 1px; }

/* a surface: everything that opens */
.surface { background: @surface; border: none; border-radius: 10px; padding: 14px; }
.surface label { font-size: 10pt; }
/* a popup grown out of the bar: the body without its top edge, which runs from its corners to the tab */
.surface.attached { border-radius: 0 0 10px 10px; padding-top: 4px; }
.edge { background: @surface; border-top-left-radius: 10px; min-height: 10px; }
.edge-right { background: @surface; border-top-right-radius: 10px; min-height: 10px; }
.gap, .gap-mid { background: @surface; min-height: 10px; }
.toast { padding: 10px; }
.osd { padding: 12px 16px; }

/* words */
.bold { font-weight: bold; }
.title { font-weight: bold; font-size: 11pt; }
.date { font-size: 14pt; font-weight: bold; }
.dim { color: @dim; }
.error { color: @urgent; }

/* what is on a surface */
.card { background: @card; border: 1px solid @rule; border-radius: 6px; padding: 10px; }
.card.critical { border-color: @urgent; }
.toast.critical { box-shadow: inset 0 0 0 1px @urgent; }
.menu { background: @card; border: 1px solid @rule; border-radius: 6px; padding: 10px; margin-top: 4px; }
.menu-head { margin-bottom: 6px; }
.battery { background: @raised; border-radius: 6px; padding: 0 14px; min-height: 40px; }
.badge { background: @accent; color: @ink; border-radius: 6px; min-width: 32px; min-height: 32px; }
.art { border-radius: 6px; }

button.round { background: @raised; border: 1px solid @rule; border-radius: 6px; min-width: 40px; min-height: 40px; }
button.arrow, button.flat-round { border-radius: 6px; min-width: 32px; min-height: 32px; }
button.round:hover, button.arrow:hover, button.flat-round:hover, button.item:hover, button.chip:hover,
calendar > header > button:hover, .hit:hover { background: @hover; }
button.round.open, button.arrow.open, button.item.on, button.chip:checked, .hit.picked { background: @accent; }
button.round.open image, button.arrow.open image, button.item.on label, button.item.on image,
button.chip:checked label, .hit.picked { color: @ink; }
button.arrow image { transition: -gtk-icon-transform 100ms; }
button.arrow.open image { -gtk-icon-transform: rotate(90deg); }
button.item { padding: 0 10px; min-height: 34px; border-radius: 6px; }
button.connect { background: @accent; padding: 0 10px; border-radius: 6px; min-height: 30px; }
button.connect label { color: @ink; font-weight: bold; }
button.chip, togglebutton.chip { background: @raised; border: 1px solid @rule; border-radius: 6px; padding: 4px 10px; min-height: 0; }

.toggle { background: @raised; border: 1px solid @rule; border-radius: 6px; min-height: 48px; }
.toggle.on { background: @accent; border-color: @accent; }
.toggle.on label, .toggle.on image { color: @ink; }
.toggle.on .toggle-sub { color: @rule; }
.toggle-main { padding: 0 6px 0 14px; border-radius: 6px; }
.toggle-main:hover { background: @raised; }
.toggle-title { font-weight: bold; }
.toggle-sub { color: @dim; font-size: 8.5pt; }
button.toggle-side { min-width: 40px; border-left: 1px solid @rule; border-radius: 0 6px 6px 0; }
.toggle.on button.toggle-side { border-left-color: @accent-rule; }
.toggle.on button.toggle-side.open { background: @accent-pressed; }

scale { padding: 0 4px; }
scale trough { min-height: 14px; border-radius: 6px; background: @well; }
scale trough highlight { border-radius: 6px; background: @accent; border: none; margin: 0; min-height: 14px; min-width: 0; }
scale slider { min-width: 0; min-height: 0; margin: 0; background: none; box-shadow: none; border: none; }
progressbar.progress trough { min-height: 3px; border-radius: 2px; background: @well; }
progressbar.progress progress { min-height: 3px; border-radius: 2px; background: @accent; }
entry, passwordentry { background: @sunk; border: 1px solid @rule; border-radius: 6px; min-height: 30px; padding: 0 8px; }
entry:focus-within, passwordentry:focus-within { border-color: @accent; }

calendar { background: @card; border: 1px solid @rule; border-radius: 6px; padding: 6px; }
calendar > header { border: none; }
calendar > header > button { min-width: 28px; min-height: 28px; border-radius: 6px; }
calendar > grid > label.day-name { color: @dim; font-weight: bold; font-size: 8.5pt; }
calendar > grid > label.day-number { padding: 6px; border-radius: 6px; }
calendar > grid > label.day-number.other-month { color: @dim; }
calendar > grid > label.day-number:selected { background: @accent; color: @ink; font-weight: bold; }

/* the tray's menus: a surface of items, a line between groups */
.tray-menu { padding: 6px; }
.tray-menu separator { background: @well; margin: 4px 6px; }

/* the bar: its black laid by its parts, not under the whole window. A block's slot is black but while the block
   is hovered or a tab: then its ground is a surface's, straight over the wallpaper (the same grey, the same
   blur), and the black left round its corners is its shadow, clipped to the slot. A block keeps its border
   (transparent) and margins either way, so nothing in the bar moves as a popup opens */
.bar-bg, .slot { background: @bar; }
.slot:hover, .slot.tab { background: transparent; }
.pill { padding: 0 8px; margin: 2px 0 0 0; border: 1px solid transparent; border-bottom-width: 0;
        border-radius: 6px 6px 0 0; transition: background 100ms; }
.slot:hover > .pill, .slot.tab > .pill { background: @surface; box-shadow: 0 0 0 30px @bar; }
.tray-item { padding: 0 5px; }
.dot { min-width: 8px; min-height: 8px; border-radius: 4px; background: @idle; transition: background 100ms; }
.dot.focused, .dot:hover { background: @accent; }
/* the launcher in it */
text.query { background: none; border: none; box-shadow: none; padding: 0; }
.hit { padding: 0 10px; }

/* the lock screen */
window.lock { background: @lock; }
.lock-time { font-size: 64pt; }
passwordentry.lock-entry { background: @lock; border: 1px solid @accent; border-radius: 6px; min-width: 320px; min-height: 40px; }
passwordentry.lock-entry:disabled { border-color: @dim; }
"#;

/// The CSS on the display, the Adwaita icons, the bar's black reloaded as bin/theme changes it.
pub fn load() {
    let Some(display) = gtk4::gdk::Display::default() else { return };
    let css = gtk4::CssProvider::new();
    let alpha_file = home().join(".cache/theme/alpha");
    let load = {
        let (css, f) = (css.clone(), alpha_file.clone());
        move || {
            let alpha = std::fs::read_to_string(&f).unwrap_or("1".into());
            css.load_from_string(&CSS.replace("ALPHA", alpha.trim()));
        }
    };
    load();
    gtk4::style_context_add_provider_for_display(&display, &css, 900);
    if let Ok(mon) = gio::File::for_path(&alpha_file).monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
        mon.connect_changed(move |_, _, _, _| load());
        // kept for the program's life
        std::mem::forget(mon);
    }
    gtk4::IconTheme::for_display(&display).set_theme_name(Some("Adwaita"));
}

/// A label set to the left, of a class ("" for none).
pub fn label(text: &str, class: &str) -> gtk4::Label {
    let l = gtk4::Label::new(Some(text));
    l.set_xalign(0.0);
    if !class.is_empty() {
        l.add_css_class(class);
    }
    l
}

/// A box emptied, to be filled anew.
pub fn clear(b: &impl IsA<gtk4::Widget>) {
    while let Some(c) = b.first_child() {
        c.unparent();
    }
}
