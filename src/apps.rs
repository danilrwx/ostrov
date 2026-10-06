//! ostrov's look in the other apps, as the appearance changes ([appearance] apps, on unless false):
//! - the colour scheme and the accent in GNOME's settings (org.gnome.desktop.interface color-scheme, accent-color),
//!   which the settings portal hands to GTK, libadwaita, Qt (Telegram), Chrome and Firefox;
//! - the palette for GTK 3 and 4 (file managers, GNOME's apps): ostrov.css beside gtk.css in ~/.config/gtk-3.0 and
//!   gtk-4.0, gtk.css importing it (a line put at its top, the file made if there is none);
//! - colour files for the rest in ~/.local/state/ostrov/colors (alacritty.toml, kitty.conf, foot.ini,
//!   telegram.tdesktop-palette, colors.sh, colors.json) and the user's own templates rendered beside them:
//!   ~/.config/ostrov/templates/NAME, its {{fg}}, {{accent}}... filled in, to colors/NAME. kitty is told to read its
//!   config again; alacritty reads an import as it changes.
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

    /// A template with its {{name}}s filled in, an unknown one left as it is.
    pub fn fill(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (k, v) in &self.colors {
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

/// libadwaita's and adw-gtk3's named colours, Adwaita's GTK 3 ones beside them, and GTK 4.16's variables.
fn gtk_css(p: &Palette) -> String {
    let defs = [
        ("accent_bg_color", "accent"), ("accent_color", "accent"), ("accent_fg_color", "ink"),
        ("window_bg_color", "bg"), ("window_fg_color", "fg"), ("view_bg_color", "view"), ("view_fg_color", "fg"),
        ("headerbar_bg_color", "bg"), ("headerbar_fg_color", "fg"), ("sidebar_bg_color", "sidebar"),
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
    out += ":root {\n";
    for (k, v) in defs {
        out += &format!("  --{}: {};\n", k.trim_end_matches("_color").replace('_', "-") + "-color", p.get(v));
    }
    out + "}\n"
}

/// What ostrov writes for the apps that read a colour file: name → its template.
const BUILT_IN: &[(&str, &str)] = &[
    ("alacritty.toml", "# ostrov's colours, its ground the bar's, as solid: import = [\"~/.local/state/ostrov/colors/alacritty.toml\"]\n[window]\nopacity = {{bar_alpha}}\n[colors.primary]\nbackground = \"{{bar}}\"\nforeground = \"{{fg}}\"\n[colors.cursor]\ncursor = \"{{accent}}\"\ntext = \"{{ink}}\"\n[colors.selection]\nbackground = \"{{accent}}\"\ntext = \"{{ink}}\"\n[colors.normal]\nblack = \"{{ansi0}}\"\nred = \"{{ansi1}}\"\ngreen = \"{{ansi2}}\"\nyellow = \"{{ansi3}}\"\nblue = \"{{ansi4}}\"\nmagenta = \"{{ansi5}}\"\ncyan = \"{{ansi6}}\"\nwhite = \"{{ansi7}}\"\n[colors.bright]\nblack = \"{{ansi8}}\"\nred = \"{{ansi9}}\"\ngreen = \"{{ansi10}}\"\nyellow = \"{{ansi11}}\"\nblue = \"{{ansi12}}\"\nmagenta = \"{{ansi13}}\"\ncyan = \"{{ansi14}}\"\nwhite = \"{{ansi15}}\"\n"),
    ("kitty.conf", "# ostrov's colours: include ~/.local/state/ostrov/colors/kitty.conf\nbackground {{bar}}\nbackground_opacity {{bar_alpha}}\nforeground {{fg}}\ncursor {{accent}}\nselection_background {{accent}}\nselection_foreground {{ink}}\nactive_border_color {{accent}}\nactive_tab_background {{accent}}\nactive_tab_foreground {{ink}}\ninactive_tab_background {{card}}\ninactive_tab_foreground {{dim}}\ncolor0 {{ansi0}}\ncolor1 {{ansi1}}\ncolor2 {{ansi2}}\ncolor3 {{ansi3}}\ncolor4 {{ansi4}}\ncolor5 {{ansi5}}\ncolor6 {{ansi6}}\ncolor7 {{ansi7}}\ncolor8 {{ansi8}}\ncolor9 {{ansi9}}\ncolor10 {{ansi10}}\ncolor11 {{ansi11}}\ncolor12 {{ansi12}}\ncolor13 {{ansi13}}\ncolor14 {{ansi14}}\ncolor15 {{ansi15}}\n"),
    ("foot.ini", "# ostrov's colours: [main] include=~/.local/state/ostrov/colors/foot.ini\n[colors]\nalpha={{bar_alpha}}\nbackground={{bar}}\nforeground={{fg}}\nselection-background={{accent}}\nselection-foreground={{ink}}\nregular0={{ansi0}}\nregular1={{ansi1}}\nregular2={{ansi2}}\nregular3={{ansi3}}\nregular4={{ansi4}}\nregular5={{ansi5}}\nregular6={{ansi6}}\nregular7={{ansi7}}\nbright0={{ansi8}}\nbright1={{ansi9}}\nbright2={{ansi10}}\nbright3={{ansi11}}\nbright4={{ansi12}}\nbright5={{ansi13}}\nbright6={{ansi14}}\nbright7={{ansi15}}\n"),
    ("telegram.tdesktop-palette", "// ostrov's colours for Telegram Desktop: Settings → Chat Settings → the three dots → Create new theme, or open this file in Telegram\nwindowBg: {{bg}};\nwindowFg: {{fg}};\nwindowBgOver: {{card}};\nwindowBgRipple: {{card}};\nwindowSubTextFg: {{dim}};\nwindowActiveTextFg: {{accent}};\nwindowBgActive: {{accent}};\nwindowFgActive: {{ink}};\nactiveButtonBg: {{accent}};\nactiveButtonFg: {{ink}};\ndialogsBg: {{sidebar}};\ndialogsBgOver: {{card}};\ndialogsBgActive: {{accent}};\ndialogsNameFg: {{fg}};\ndialogsTextFg: {{dim}};\ndialogsNameFgActive: {{ink}};\ndialogsTextFgActive: {{ink}};\ntitleBg: {{bg}};\ntitleFg: {{fg}};\nsideBarBg: {{sidebar}};\nhistoryComposeAreaBg: {{view}};\nmsgInBg: {{card}};\nmsgOutBg: {{accent}};\nmsgOutFg: {{ink}};\nmsgInFg: {{fg}};\n"),
    ("colors.sh", "# ostrov's colours for scripts: . ~/.local/state/ostrov/colors/colors.sh\nBG='{{bg}}'\nFG='{{fg}}'\nDIM='{{dim}}'\nACCENT='{{accent}}'\nINK='{{ink}}'\nURGENT='{{urgent}}'\n"),
];

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
}

/// Everything above, as the appearance has it now (style.rs on every config change).
pub fn apply(a: &Appearance, t: &Theme, colors: &BTreeMap<String, String>) {
    if !a.apps {
        return;
    }
    let p = Palette::of(a, t, colors);
    gnome(&p);
    let css = gtk_css(&p);
    for v in ["gtk-3.0", "gtk-4.0"] {
        let dir = home().join(".config").join(v);
        put(&dir.join("ostrov.css"), &css);
        import(&dir);
    }
    let out = home().join(".local/state/ostrov/colors");
    let mut kitty = false;
    for (name, text) in BUILT_IN {
        kitty |= put(&out.join(name), &p.fill(text)) && *name == "kitty.conf";
    }
    let json: BTreeMap<_, _> = p.colors.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
    put(&out.join("colors.json"), &(serde_json::to_string_pretty(&json).unwrap_or_default() + "\n"));
    // the user's own, by their names
    if let Ok(dir) = std::fs::read_dir(home().join(".config/ostrov/templates")) {
        for e in dir.flatten().filter(|e| e.path().is_file()) {
            if let Ok(text) = std::fs::read_to_string(e.path()) {
                put(&out.join(e.file_name()), &p.fill(&text));
            }
        }
    }
    // kitty reads its config again on SIGUSR1; alacritty and foot as their files change or at their start
    if kitty {
        let _ = std::process::Command::new("pkill").args(["-USR1", "-x", "kitty"]).status();
    }
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
        assert_eq!(p.fill("a {{accent}} {{nope}}"), "a #8b7cf6 {{nope}}");
        assert!(gtk_css(&p).contains("@define-color accent_bg_color #8b7cf6;"));
        assert!(gtk_css(&p).contains("--accent-bg-color: #8b7cf6;"));
    }
}
