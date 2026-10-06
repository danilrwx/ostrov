//! How ostrov looks, all in one place: the palette (every colour a name, set here alone), the shapes, the CSS
//! every part draws with, and the few widget helpers they share. The bar as solid as [appearance]'s bar_opacity
//! says, else see-through over a wallpaper and solid over none, reloaded as the wallpaper's pick changes.

use gtk4::prelude::*;
use gtk4::gio;


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
@define-color surface-solid #000000;            /* the surface without its see-through, under a hover */
@define-color hover rgba(255, 255, 255, 0.15);  /* under the pointer */
@define-color raised rgba(255, 255, 255, 0.08); /* a button, a toggle, a chip on a surface */
@define-color card rgba(255, 255, 255, 0.06);   /* a card, a menu, the month on a surface */
@define-color well rgba(255, 255, 255, 0.1);    /* a trough, a line inside a surface */
@define-color sunk rgba(0, 0, 0, 0.2);          /* an entry */
@define-color rule rgba(255, 255, 255, 0.1);   /* an outline, see-through as the ground under it is */
@define-color idle #666666;                     /* a workspace not focused */
@define-color handle #3b82f6;                   /* a tile's corner in Edit, sizing it */
@define-color urgent #cd0000;                   /* an error, a critical notification */
@define-color recording #ff4040;                /* the mic or the camera taken (bar/privacy.rs) */
@define-color lock #000000;                     /* the lock screen */
@define-color ground #000000;                   /* under the wallpaper, and in its place with none */
@define-color shade rgba(0, 0, 0, 0.4);         /* the screen under a question (polkit's, askpass's) */
@define-color scrim rgba(0, 0, 0, 0.55);        /* under a button over a picture (play on the player's cover) */
@define-color on-scrim #ffffff;                 /* an icon on the scrim and on the handle */

/* the shapes: a surface rounded 10, what is on it 6 */
* { font-size: 11pt; color: @fg; }
image { -gtk-icon-size: 16px; }
window { background: transparent; }
scrolledwindow { background: none; }
button { background: none; border: none; box-shadow: none; outline: none; min-height: 0; min-width: 0; padding: 0; }
/* the keyboard's focus seen (Tab through a question's password and buttons), the pointer's clicks not ringed */
button:disabled { opacity: 0.45; }
button:focus-visible, entry:focus-visible, passwordentry:focus-visible { outline: 2px solid @accent; outline-offset: 2px; }
/* what the pointer or a click changes changes over a moment, not at once (GTK's animations off: at once) */
button, .toggle, .slot, .pill, .tile > :first-child, entry, passwordentry {
    transition: background-color 150ms ease-out, color 150ms ease-out, border-color 150ms ease-out,
                box-shadow 150ms ease-out, opacity 150ms ease-out; }
label, image { transition: color 150ms ease-out; }
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
.bold { font-weight: 600; }
.title { font-weight: 600; font-size: 11pt; }
.date { font-size: 14pt; font-weight: 600; }
.dim { color: @dim; }
.error { color: @urgent; }

/* what is on a surface */
/* a card, outlined: in a menu on the card's ground, a tile's on a raised one as a toggle's */
.card, .menu, .battery, .plugin-card, .cc .card, .cc .clock { border: 1px solid @rule; border-radius: 6px; }
.card, .menu { background: @card; padding: 10px; }
.battery, .plugin-card, .cc .card, .cc .clock { background: @raised; }
.menu { margin-top: 4px; }
.battery { padding: 0 14px; min-height: 40px; }
/* a plugin's widget drawn as anything but a toggle, a slider or a round button: on a card like the battery's */
.plugin-card { padding: 4px 14px; }
.card.critical { border-color: @urgent; }
.toast.critical { box-shadow: inset 0 0 0 1px @urgent; }
.menu-head { margin-bottom: 6px; }
.badge { background: @accent; color: @ink; border-radius: 6px; min-width: 32px; min-height: 32px; }
.art { border-radius: 6px; }
/* play/pause over the art, the player two cells wide */
button.art-play { background: @scrim; min-width: 40px; min-height: 40px; border-radius: 9999px; padding: 0; }
button.art-play image { color: @on-scrim; }

button.round { background: @raised; border: 1px solid @rule; border-radius: 6px; min-width: 40px; min-height: 40px; }
button.arrow, button.flat-round { border-radius: 6px; min-width: 32px; min-height: 32px; }
button.round:hover, button.arrow:hover, button.flat-round:hover, button.item:hover, button.chip:hover,
calendar > header > button:hover, .hit:hover { background: @hover; }
button.round.open, button.arrow.open, .tile.open button.round, .tile.open button.arrow:not(.toggle-side), button.item.on, .hit.picked { background: @accent; }
button.chip:checked, button.chip.picked { background: @accent; border-color: @accent; }
button.round.open image, button.arrow.open image, .tile.open button.round image,
.tile.open button.arrow:not(.toggle-side) image, button.item.on label, button.item.on image,
button.chip:checked label, button.chip:checked image, button.chip.picked label, .hit.picked label, .hit.picked image { color: @ink; }
button.arrow image { transition: -gtk-icon-transform 100ms; }
button.item { padding: 0 10px; min-height: 34px; border-radius: 6px; }
button.connect { background: @accent; padding: 0 10px; border-radius: 6px; min-height: 30px; }
button.connect label { color: @ink; font-weight: 600; }
/* a message's send: a square on the accent, as tall as the entry beside it */
button.send { background: @accent; border-radius: 6px; min-width: 32px; min-height: 32px; }
button.send:hover { background: @accent-pressed; }
button.send image { color: @ink; -gtk-icon-size: 16px; }
button.chip, togglebutton.chip { background: @raised; border: 1px solid @rule; border-radius: 6px; padding: 4px 10px; min-height: 0; }

.toggle { background: @raised; border: 1px solid @rule; border-radius: 6px; min-height: 48px; }
.toggle.on { background: @accent; border-color: @accent; }
.toggle.on label, .toggle.on image { color: @ink; }
.toggle.on .toggle-sub { color: alpha(@ink, 0.72); }
.toggle-main { padding: 0 6px 0 14px; border-radius: 6px; }
/* with its arrow beside it, square where they meet */
.toggle-main:not(:last-child) { border-radius: 6px 0 0 6px; }
.toggle-main:hover { background: @raised; }
.toggle-title { font-weight: 600; }
.toggle-glyph { font-weight: 600; min-width: 16px; }
.toggle-sub { color: @dim; font-size: 8.5pt; }
button.toggle-side { min-width: 40px; border-left: 1px solid @rule; border-radius: 0 6px 6px 0; }
.toggle.on button.toggle-side { border-left-color: alpha(@ink, 0.22); }
.toggle.on button.toggle-side.open, .tile.open .toggle.on button.toggle-side { background: @accent-pressed; }
.toggle.small .toggle-main { padding: 0; }

/* the control centre's grid (cc/): its tiles, and in its editing their minus and corner, the one dragged lifted */
.cc .battery { min-height: 0; }
/* every tile of a panel on the same ground as a toggle's */
.cc .month calendar { background: none; border: none; padding: 0; }
.cc scrolledwindow.card, .cc scrolledwindow.clock { padding: 0; }
.cc .card-body { padding: 10px; }
.surface label.hour { color: @dim; font-size: 8.5pt; }
.cc .clock-body { padding: 0 14px; }
.cc.editing .tile > :first-child { opacity: 0.85; }
.cc.editing { margin: 6px; }
/* the bar's editor (bar/edit.rs): its parts outlined, the picked block ringed, where a drop lands */
.editing .zone { border: 1px dashed @dim; border-radius: 6px; min-width: 48px; }
.editing .edit-picked { box-shadow: inset 0 0 0 2px @handle; border-radius: 6px; }
.drop-marker { background: @handle; min-width: 3px; border-radius: 2px; margin: 4px 2px; }
.bar-gallery { padding: 12px 16px; }
.gallery-item { padding: 4px 10px; }
/* a line of a panel's own in its Edit (its width), as a menu's row is set */
.gallery-line { padding: 0 10px; min-height: 34px; }
/* a hover in an open popup, drawn in the bar's window (popup.rs), as GTK's tooltips look */
.bubble { background: @surface-solid; color: @fg; border: 1px solid @rule; border-radius: 6px; padding: 2px 8px;
          font-weight: 600; }
/* the dots on a slider's steps, over its track */
.slider-dots { color: alpha(@fg, 0.55); }
.slider-dots.filled { color: alpha(@ink, 0.6); }
.slider-value { min-width: 36px; }
.hover-tip { background: @surface-solid; color: @fg; border: 1px solid @rule; border-radius: 6px; padding: 4px 8px; }
.tile.dragged > :first-child { box-shadow: 0 0 0 2px @accent; border-radius: 6px; }
button.tile-remove { background: @urgent; border-radius: 9px; min-width: 18px; min-height: 18px; margin: -4px; }
button.tile-remove image { color: @fg; -gtk-icon-size: 12px; }
/* a widget showing nothing now, in the editing: its icon and name on a dashed outline */
.tile-ghost { border: 1px dashed @dim; border-radius: 6px; padding: 0 14px; }
.tile.picked > :first-child { box-shadow: 0 0 0 2px @accent; border-radius: 6px; }
.tile-grip { background: @handle; color: @on-scrim; border-radius: 10px; min-width: 20px; min-height: 20px; margin: -5px; -gtk-icon-size: 12px; }
.clock { padding: 0 14px; }
.surface label.clock-time { font-size: 20pt; font-weight: 600; }
/* the tiles as tall as the density's rows (look.rs), not their own */
.cc .tile .toggle, .cc .tile button.round { min-height: 0; min-width: 0; }

/* the control centre's pages (Appearance, Settings): a header with its back arrow; a form's fields, the title
   over its help, the control beside or under them; the themes' cards, the accents' swatches */
.page-title { font-weight: 600; font-size: 12pt; }
.form-section { margin-top: 10px; }
.field { padding: 6px 0; }
.field-help { color: @dim; font-size: 8.5pt; }
flowboxchild { padding: 0; }
.page-body { margin-right: 10px; }
scrollbar { background: none; border: none; }
scrollbar slider { background: @well; border: none; border-radius: 999px; min-width: 4px; min-height: 24px; }
button.theme { border: 2px solid @rule; border-radius: 6px; min-height: 40px; padding: 4px 6px; }
button.theme label { font-size: 9pt; }
button.theme.picked { border-color: @accent; }
.theme-dot { min-width: 10px; min-height: 10px; border-radius: 5px; }
button.swatch { border: 2px solid @rule; border-radius: 999px; min-width: 20px; min-height: 20px; }
button.swatch.picked { border-color: @fg; }
colordialogbutton > button { background: @raised; border: 1px solid @rule; border-radius: 6px; padding: 3px; }
colordialogbutton.swatch-custom > button { border: 2px solid @rule; border-radius: 999px; padding: 2px; }
colordialogbutton.swatch-custom.picked > button { border-color: @fg; }
colordialogbutton.swatch-custom colorswatch { border-radius: 999px; min-width: 18px; min-height: 18px; }
switch { background: @well; border: none; border-radius: 999px; min-width: 40px; min-height: 22px; }
switch:checked { background: @accent; }
switch slider { background: @fg; border: none; border-radius: 999px; min-width: 18px; min-height: 18px; margin: 2px;
                box-shadow: none; }
switch:checked slider { background: @ink; }
switch image { color: transparent; }
spinbutton { background: @sunk; border: 1px solid @rule; border-radius: 6px; }
spinbutton > text { padding: 0 6px; }
spinbutton > button { min-width: 26px; border-radius: 6px; }
spinbutton > button:hover { background: @hover; }
scale value { color: @dim; font-size: 9pt; }

scale { padding: 0 4px; }
scale trough { min-height: 14px; border-radius: 6px; background: @well; border: none; }
scale trough highlight { border-radius: 6px; background: @accent; border: none; margin: 0; min-height: 14px; min-width: 0; }
scale slider { min-width: 0; min-height: 0; margin: 0; background: none; box-shadow: none; border: none; }
scale:focus-visible trough { outline: 2px solid @accent; outline-offset: 2px; }
/* as narrow as its tile: GTK's own progress bar asks 150px */
progressbar.progress, progressbar.progress trough, progressbar.progress progress { min-width: 0; }
progressbar.progress trough { min-height: 3px; border-radius: 2px; background: @well; }
progressbar.progress progress { min-height: 3px; border-radius: 2px; background: @accent; }
entry, passwordentry { background: @sunk; border: 1px solid @rule; border-radius: 6px; min-height: 30px; padding: 0 8px; }
entry:focus-within, passwordentry:focus-within { border-color: @accent; }

calendar { background: @card; border: 1px solid @rule; border-radius: 6px; padding: 6px; }
calendar > header { border: none; }
calendar > header > button { min-width: 28px; min-height: 28px; border-radius: 6px; }
calendar > grid > label.day-name { color: @dim; font-weight: 600; font-size: 8.5pt; }
calendar > grid > label.day-number { padding: 6px; border-radius: 6px; }
calendar > grid > label.day-number.other-month { color: @dim; }
/* a month two cells of the calendar wide: its days closer and smaller, their names and the year's switch gone
   (calendar/widget.rs; the month's arrows go on past December) */
calendar.narrow { padding: 2px; }
calendar.narrow > grid > label.day-number { padding: 2px 1px; font-size: 8pt; }
calendar > grid > label.day-number:selected { background: @accent; color: @ink; font-weight: 600; }

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
        border-radius: 6px 6px 0 0; }
.slot:hover > .pill, .slot.tab > .pill { background: @surface; box-shadow: 0 0 0 30px @bar; }
.tray-item { padding: 0 5px; }
.recording image { color: @recording; }
.dot { min-width: 8px; min-height: 8px; border-radius: 4px; background: @idle; transition: background 100ms; }
.dot.focused, .dot:hover { background: @accent; }
/* the launcher in it */
text.query { background: none; border: none; box-shadow: none; padding: 0; }
.hit { padding: 0 10px; }
/* the clipboard entry picked, whole: its text in the terminal's font, its picture at most so big */
.preview { padding: 10px; }
.preview label { font-size: 10pt; }
.preview picture { min-width: 0; min-height: 0; }

window.wallpaper { background: @ground; }
window.prompt { background: @shade; }

/* the lock screen */
window.lock { background: @lock; }
.lock-time { font-size: 64pt; }
.lock-entry { background: @lock; border: 1px solid @accent; border-radius: 6px; min-width: 320px; min-height: 40px; }
.lock-entry:disabled { border-color: @dim; }
"#;

/// [appearance] sliders = "thin": the track a line, its knob round on it.
const THIN: &str = "
scale trough { min-height: 4px; border-radius: 999px; }
scale trough highlight { min-height: 4px; border-radius: 999px; }
scale slider { min-width: 16px; min-height: 16px; margin: -6px 0; border-radius: 999px; background: @accent; }
.slider-dots { color: @dim; }
.slider-dots.filled { color: @accent; }
";

thread_local! {
    /// What follows the config as it changes, past the CSS (the control centre's density, the Settings' forms).
    static WATCHERS: std::cell::RefCell<Vec<Box<dyn Fn()>>> = Default::default();
    /// The CSS loaded anew, once load() has set it.
    static RELOAD: std::cell::RefCell<Option<std::rc::Rc<dyn Fn()>>> = Default::default();
}

/// The palette's tokens, the names a theme's [colors] and [colors] may set.
pub fn tokens() -> Vec<&'static str> {
    CSS.lines().filter_map(|l| l.strip_prefix("@define-color ")?.split_whitespace().next()).collect()
}

/// The CSS loaded anew as the files say now: a theme installed, removed or edited.
pub fn reload() {
    if let Some(f) = RELOAD.with(|r| r.borrow().clone()) {
        f();
    }
}

/// f on every change to the config (and to the wallpaper's pick), once the CSS has followed it.
pub fn on_config(f: impl Fn() + 'static) {
    WATCHERS.with(|w| w.borrow_mut().push(Box::new(f)));
}

/// The CSS on the display, the Adwaita icons, the bar's black reloaded as the wallpaper's pick changes; the look as
/// config.toml's [appearance] sets it (look.rs), its theme's (theme.rs) palette and theme.css.
pub fn load() {
    let Some(display) = gtk4::gdk::Display::default() else { return };
    let css = gtk4::CssProvider::new();
    let alpha_file = crate::modules::wallpaper::service::file();
    let load = {
        let css = css.clone();
        move || {
            let alpha = crate::modules::wallpaper::service::bar_alpha();
            // the theme's palette, the appearance's and the config's colours over it, before the rules that take
            // them; the theme's own CSS last
            let cfg = crate::config::load();
            let theme = crate::theme::get(&cfg.appearance.theme);
            let look = crate::look::palette(&cfg.appearance, &theme, &cfg.colors);
            let (palette, rules) = CSS.split_at(CSS.find("/* the shapes").unwrap_or(0));
            let rules = crate::look::radii(rules, crate::look::radius(&cfg.appearance, &theme));
            let rules = crate::look::sizes(&rules, &cfg.appearance);
            let previews = crate::look::previews(&crate::theme::all());
            // a tab's ground over the wallpaper matches a blurred panel; without blur the bar's is the nearer
            let a = &cfg.appearance;
            let unblurred = if a.tab == "bar" || a.tab != "wallpaper" && a.blur.or(theme.blur) == Some(false) {
                ".slot:hover, .slot.tab { background: @bar; }\n"
            } else {
                ""
            };
            let sliders = if cfg.appearance.sliders == "thin" { THIN } else { "" };
            let all = format!("{palette}{look}{rules}{unblurred}{sliders}{previews}{}", theme.css);
            css.load_from_string(&all.replace("ALPHA", &alpha.to_string()));
            crate::look::apply(&cfg.appearance, &theme);
            WATCHERS.with(|w| w.borrow().iter().for_each(|f| f()));
        }
    };
    load();
    RELOAD.with(|r| *r.borrow_mut() = Some(std::rc::Rc::new(load.clone())));
    gtk4::style_context_add_provider_for_display(&display, &css, 900);
    for f in [alpha_file, crate::config::path()] {
        if let Ok(mon) = gio::File::for_path(&f).monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
            let load = load.clone();
            mon.connect_changed(move |_, _, _, _| load());
            // kept for the program's life
            std::mem::forget(mon);
        }
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
