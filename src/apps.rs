//! ostrov's look in the other apps, as the appearance changes ([appearance] apps, on unless false):
//! - the colour scheme and the accent in GNOME's settings (org.gnome.desktop.interface color-scheme, accent-color),
//!   which the settings portal hands to GTK, libadwaita, Qt (Telegram), Chrome and Firefox;
//! - the palette for GTK 3 and 4 (file managers, GNOME's apps): ostrov.css beside gtk.css in ~/.config/gtk-3.0 and
//!   gtk-4.0, gtk.css importing it (a line put at its top, the file made if there is none);
//! - colour files for the rest in ~/.local/state/ostrov/colors, an app's each (integrations.rs: terminals, k9s,
//!   Telegram, the user's own), and colors.json, the palette for scripts;
//! - the browser's frame the bar's colour: GTK's headerbar is the bar's, which Chrome in its GTK theme takes; its
//!   theme colour policy (ostrov-theme.json, where it is there to write) left empty, as it would hold over it.
//!
//! Each written only when what it says changed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gtk4::gio;
use gtk4::prelude::*;

use crate::config::Appearance;
use crate::theme::Theme;

/// The palette as the other apps take it, every colour solid, `#rrggbb`.
pub struct Palette {
    pub dark: bool,
    /// name → colour: bg (a window's ground), view (a list's, a text's), card, sidebar, fg, dim, accent, ink (on the
    /// accent), urgent, bar (the bar's colour) and bar_alpha (how solid it is, 0 to 1), and the terminal's ansi0 to
    /// ansi15
    pub colors: BTreeMap<&'static str, String>,
}

fn rgb(c: &str) -> Option<(u8, u8, u8)> {
    let c = gtk4::gdk::RGBA::parse(c).ok()?;
    let b = |x: f32| (x * 255.0).round() as u8;
    Some((b(c.red()), b(c.green()), b(c.blue())))
}

fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// a with a share k of b mixed in
fn mix(a: (u8, u8, u8), b: (u8, u8, u8), k: f64) -> (u8, u8, u8) {
    let m = |x: u8, y: u8| (x as f64 * (1.0 - k) + y as f64 * k).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// The terminal's sixteen colours: One Dark's and One Light's, the blues the accent's.
const ANSI_DARK: [&str; 16] = [
    "#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf",
    "#5c6370", "#e88389", "#a9d18e", "#ecc98f", "#7cbcf2", "#d08fe4", "#6ec6d0", "#dcdfe4",
];
const ANSI_LIGHT: [&str; 16] = [
    "#383a42", "#e45649", "#50a14f", "#c18401", "#0184bc", "#a626a4", "#0997b3", "#fafafa",
    "#4f525e", "#e06c75", "#61b260", "#d19a2a", "#2f99c9", "#b955b7", "#28a8c4", "#ffffff",
];

impl Palette {
    /// As the appearance, its theme and the config's [colors] have it, over style.rs's own.
    pub fn of(a: &Appearance, t: &Theme, colors: &BTreeMap<String, String>) -> Palette {
        let pick = |k: &str, own: &str| {
            colors.get(k).or(t.colors.get(k)).and_then(|c| rgb(c)).unwrap_or_else(|| rgb(own).unwrap_or((0, 0, 0)))
        };
        let dark = t.dark;
        let surface = rgb(&a.surface).or_else(|| t.colors.get("surface").and_then(|s| rgb(s))).unwrap_or((0, 0, 0));
        // with no wallpaper the windows' ground the bar's colour, the desktop one colour from the bar down; over a
        // picture the panels' surface
        let bar = rgb(&a.bar_color).or_else(|| t.colors.get("bar").and_then(|b| rgb(&b.replace("ALPHA", "1")))).unwrap_or((0, 0, 0));
        let surface = if crate::modules::wallpaper::service::pick().on { surface } else { bar };
        let bar_alpha = crate::modules::wallpaper::service::bar_alpha();
        let fg = pick("fg", if dark { "#ffffff" } else { "#1d1d1f" });
        let dim = pick("dim", if dark { "#888888" } else { "#505055" });
        let (accent, ink) = match rgb(&a.accent) {
            Some(c) => (c, if 0.2126 * c.0 as f64 + 0.7152 * c.1 as f64 + 0.0722 * c.2 as f64 > 150.0 { (0, 0, 0) } else { (255, 255, 255) }),
            None => (pick("accent", "#ffffff"), pick("ink", "#000000")),
        };
        let urgent = pick("urgent", "#cd0000");
        let mut c = BTreeMap::new();
        c.insert("bg", hex(surface));
        // the bar's colour and how solid it is: a terminal's ground, the same as the bar's above it
        c.insert("bar", hex(bar));
        c.insert("bar_alpha", format!("{bar_alpha:.2}"));
        c.insert("sidebar", hex(mix(surface, fg, 0.03)));
        c.insert("view", hex(mix(surface, fg, 0.05)));
        c.insert("card", hex(mix(surface, fg, 0.08)));
        c.insert("fg", hex(fg));
        c.insert("dim", hex(dim));
        c.insert("accent", hex(accent));
        c.insert("ink", hex(ink));
        c.insert("urgent", hex(urgent));
        const NAMES: [&str; 16] = [
            "ansi0", "ansi1", "ansi2", "ansi3", "ansi4", "ansi5", "ansi6", "ansi7", "ansi8", "ansi9", "ansi10",
            "ansi11", "ansi12", "ansi13", "ansi14", "ansi15",
        ];
        for (n, v) in NAMES.iter().zip(if dark { ANSI_DARK } else { ANSI_LIGHT }) {
            c.insert(n, v.to_string());
        }
        Palette { dark, colors: c }
    }

    fn get(&self, k: &str) -> &str {
        self.colors.get(k).map_or("#000000", String::as_str)
    }

    /// A template with its {{name}}s filled in ({{name|bare}}: without its #, as foot writes a colour), an unknown
    /// one left as it is.
    pub fn fill(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (k, v) in &self.colors {
            out = out.replace(&format!("{{{{{k}|bare}}}}"), v.trim_start_matches('#'));
            out = out.replace(&format!("{{{{{k}}}}}"), v);
        }
        out
    }

    /// GNOME's accent names nearest the accent: its hue among theirs, slate for a grey.
    pub fn accent_name(&self) -> &'static str {
        let (r, g, b) = rgb(self.get("accent")).unwrap_or((0, 0, 0));
        let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        if max - min < 0.15 {
            return "slate";
        }
        let d = max - min;
        let h = if max == r { 60.0 * (((g - b) / d).rem_euclid(6.0)) } else if max == g { 60.0 * ((b - r) / d + 2.0) } else { 60.0 * ((r - g) / d + 4.0) };
        let named = [("red", 353.0), ("orange", 23.0), ("yellow", 41.0), ("green", 130.0), ("teal", 189.0), ("blue", 213.0), ("purple", 270.0), ("pink", 330.0)];
        let dist = |x: f64| ((h - x).abs() % 360.0).min(360.0 - (h - x).abs() % 360.0);
        named.iter().min_by(|a, b| dist(a.1).total_cmp(&dist(b.1))).map_or("blue", |n| n.0)
    }
}

/// libadwaita's and adw-gtk3's named colours, Adwaita's GTK 3 ones beside them, and (GTK 4, vars) 4.16's variables,
/// which GTK 3 cannot read.
fn gtk_css(p: &Palette, vars: bool) -> String {
    let defs = [
        ("accent_bg_color", "accent"), ("accent_color", "accent"), ("accent_fg_color", "ink"),
        ("window_bg_color", "bg"), ("window_fg_color", "fg"), ("view_bg_color", "view"), ("view_fg_color", "fg"),
        ("headerbar_bg_color", "bar"), ("headerbar_fg_color", "fg"), ("sidebar_bg_color", "sidebar"),
        ("sidebar_fg_color", "fg"), ("card_bg_color", "card"), ("card_fg_color", "fg"), ("popover_bg_color", "view"),
        ("popover_fg_color", "fg"), ("dialog_bg_color", "view"), ("dialog_fg_color", "fg"),
        ("theme_bg_color", "bg"), ("theme_fg_color", "fg"), ("theme_base_color", "view"), ("theme_text_color", "fg"),
        ("theme_selected_bg_color", "accent"), ("theme_selected_fg_color", "ink"),
        ("destructive_bg_color", "urgent"), ("error_color", "urgent"),
    ];
    let mut out = String::from("/* written by ostrov as its appearance changes ([appearance] apps); gtk.css imports it */\n");
    for (k, v) in defs {
        out += &format!("@define-color {k} {};\n", p.get(v));
    }
    if !vars {
        return out;
    }
    out += ":root {\n";
    for (k, v) in defs {
        out += &format!("  --{}: {};\n", k.trim_end_matches("_color").replace('_', "-") + "-color", p.get(v));
    }
    out + "}\n"
}


fn home() -> PathBuf {
    crate::hub::home()
}

/// path holding text, written only if it says something else now; whether it changed.
fn put(path: &Path, text: &str) -> bool {
    if std::fs::read_to_string(path).ok().as_deref() == Some(text) {
        return false;
    }
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    match std::fs::write(path, text) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("ostrov: {}: {e}", path.display());
            false
        }
    }
}

/// gtk.css importing ostrov.css: the line at its top unless it is there, the file made if there is none.
fn import(dir: &Path) {
    let gtk = dir.join("gtk.css");
    let line = "@import url(\"ostrov.css\");";
    let now = std::fs::read_to_string(&gtk).unwrap_or_default();
    if !now.contains(line) {
        put(&gtk, &format!("{line}\n{now}"));
    }
}

/// The directories GTK's themes are found in.
fn theme_dirs() -> [PathBuf; 3] {
    [home().join(".local/share/themes"), home().join(".themes"), PathBuf::from("/usr/share/themes")]
}

/// GTK 3's theme of ostrov's own, ~/.local/share/themes/ostrov: adw-gtk3's (dark or light, as the theme is) with
/// ostrov's colours written into it. GTK 3 reads a user's gtk.css once, as an app starts, but its theme again
/// whenever the theme is named anew: so the apps running (Thunar, Chrome's frame) follow a change at once
/// (DankMaterialShell's way). None without adw-gtk3; whether it changed.
fn gtk3_theme(p: &Palette) -> Option<bool> {
    let from = if p.dark { "adw-gtk3-dark" } else { "adw-gtk3" };
    let src = theme_dirs().iter().map(|d| d.join(from).join("gtk-3.0")).find(|d| d.join("gtk.css").is_file())?;
    let dst = home().join(".local/share/themes/ostrov/gtk-3.0");
    // its assets copied once a side, from whichever adw-gtk3 there is
    let side = dst.join(".from");
    if std::fs::read_to_string(&side).ok().as_deref() != Some(from) {
        let _ = std::fs::remove_dir_all(&dst);
        copy_dir(&src, &dst).ok()?;
        put(&side, from);
    }
    let base = std::fs::read_to_string(src.join("gtk.css")).ok()?;
    let ours = gtk_css(p, false);
    let changed = put(&dst.join("gtk.css"), &format!("{base}\n/* ostrov's colours over adw-gtk3's */\n{ours}"));
    // its dark variant (an app asking for it) the same
    put(&dst.join("gtk-dark.css"), "@import url(\"gtk.css\");\n");
    put(&dst.parent()?.join("index.theme"), "[Desktop Entry]\nType=X-GNOME-Metatheme\nName=ostrov\nComment=adw-gtk3 in ostrov's colours, written by ostrov\n\n[X-GNOME-Metatheme]\nGtkTheme=ostrov\n");
    Some(changed)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

/// GTK's theme set to nothing and back, the apps running reading their CSS again (DankMaterialShell's way).
fn refresh_gtk() {
    let Some(schema) = gio::SettingsSchemaSource::default().and_then(|s| s.lookup("org.gnome.desktop.interface", true)) else { return };
    let s = gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None);
    let theme = s.string("gtk-theme");
    let _ = s.set_string("gtk-theme", "");
    gio::Settings::sync();
    let _ = s.set_string("gtk-theme", &theme);
}

/// GNOME's interface settings: the scheme and the accent, where the schema has them.
fn gnome(p: &Palette) {
    let Some(src) = gio::SettingsSchemaSource::default() else { return };
    let Some(schema) = src.lookup("org.gnome.desktop.interface", true) else { return };
    let s = gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None);
    let scheme = if p.dark { "prefer-dark" } else { "prefer-light" };
    if schema.has_key("color-scheme") && s.string("color-scheme") != scheme {
        let _ = s.set_string("color-scheme", scheme);
    }
    if schema.has_key("accent-color") && s.string("accent-color") != p.accent_name() {
        let _ = s.set_string("accent-color", p.accent_name());
    }
    // GTK 3's theme (the portal hands it on over settings.ini): ostrov's own copy of adw-gtk3 in its colours where
    // adw-gtk3 is installed (gtk3_theme), dark or light as the theme is
    if home().join(".local/share/themes/ostrov/gtk-3.0/gtk.css").is_file() && schema.has_key("gtk-theme") && s.string("gtk-theme") != "ostrov" {
        let _ = s.set_string("gtk-theme", "ostrov");
    }
}

/// Chrome's and Chromium's policy files ostrov sets the theme colour in, where one is there and this user may
/// write it (made once: `sudo install -m 644 -o $USER /dev/null .../ostrov-theme.json`, with '{}' in it).
const BROWSER_POLICIES: &[&str] =
    &["/etc/opt/chrome/policies/managed/ostrov-theme.json", "/etc/chromium/policies/managed/ostrov-theme.json"];

/// The browsers' theme colour policy left empty, in each policy file there to write: Chrome in its GTK theme takes
/// its frame from GTK's headerbar, the bar's colour exactly (a policy colour is a seed it makes its own palette
/// from, and would hold over the GTK theme).
fn browsers() {
    let files: Vec<&Path> = BROWSER_POLICIES.iter().map(Path::new).filter(|f| {
        std::fs::metadata(f).is_ok_and(|m| !m.permissions().readonly()) && std::fs::OpenOptions::new().append(true).open(f).is_ok()
    }).collect();
    if files.is_empty() {
        return;
    }
    for f in files {
        put(f, "{}\n");
    }
}

/// Everything above, as the appearance has it now (style.rs on every config change).
pub fn apply(a: &Appearance, t: &Theme, colors: &BTreeMap<String, String>) {
    if !a.apps {
        return;
    }
    let p = Palette::of(a, t, colors);
    // GTK 3's own theme before the settings name it
    let gtk3 = gtk3_theme(&p).unwrap_or(false);
    gnome(&p);
    browsers();
    let mut css = false;
    for (v, vars) in [("gtk-3.0", false), ("gtk-4.0", true)] {
        let dir = home().join(".config").join(v);
        css |= put(&dir.join("ostrov.css"), &gtk_css(&p, vars));
        import(&dir);
    }
    // the GTK apps running (Chrome's frame in its GTK theme among them) read it again: their theme named anew
    if css || gtk3 {
        refresh_gtk();
    }
    // the apps coloured by a file (integrations.rs), and the palette for scripts
    crate::integrations::apply(&p);
    let json: BTreeMap<_, _> = p.colors.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
    put(&crate::integrations::out().join("colors.json"), &(serde_json::to_string_pretty(&json).unwrap_or_default() + "\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette(accent: &str) -> Palette {
        let mut colors = BTreeMap::new();
        colors.insert("accent", accent.to_string());
        Palette { dark: true, colors }
    }

    #[test]
    fn accents_named() {
        assert_eq!(palette("#8b7cf6").accent_name(), "purple");
        assert_eq!(palette("#3584e4").accent_name(), "blue");
        assert_eq!(palette("#88c0d0").accent_name(), "teal");
        assert_eq!(palette("#ffffff").accent_name(), "slate");
        assert_eq!(palette("#e62d42").accent_name(), "red");
    }

    #[test]
    fn templates_filled() {
        let p = palette("#8b7cf6");
        assert_eq!(p.fill("a {{accent}} {{accent|bare}} {{nope}}"), "a #8b7cf6 8b7cf6 {{nope}}");
        assert!(gtk_css(&p, false).contains("@define-color accent_bg_color #8b7cf6;"));
        assert!(gtk_css(&p, true).contains("--accent-bg-color: #8b7cf6;"));
        assert!(!gtk_css(&p, false).contains(":root"));
    }
}
