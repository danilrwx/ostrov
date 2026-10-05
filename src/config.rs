//! ostrov's config, ~/.config/ostrov/config.toml: the bar's blocks, idle's times, the
//! palette's colours over style.rs's own. Every key may be left out for its default; a file that does not read is
//! said on stderr and the defaults taken. The colours follow the file as it changes; the rest is read at the start.
//!
//!     [bar]
//!     left = ["workspaces"]
//!     center = ["panel.calendar"]  # a panel: its widgets' badges, the panel unrolled out of them
//!     right = ["privacy", "layout", "tray", "panel.control"]
//!     monitors = "all"            # "primary", or the ones named: ["eDP-1", "DP-2"]
//!
//!     [idle]
//!     lock = 600          # seconds idle to the lock, 0 never
//!     screens_off = 900   # to the screens off, 0 never
//!
//!     [colors]            # any of style.rs's palette
//!     surface = "rgba(0, 0, 0, 0.75)"
//!
//!     [launcher]          # the launcher's web search (s words), {} the query
//!     search = "https://duckduckgo.com/?q={}"
//!
//!     [appearance]        # the look (style.rs), taken as the file is saved; the control centre's Appearance
//!     theme = "dark"      # dark, light, graphite, nord, solarized, or an installed one (theme.rs)
//!     accent = "#5e81ac"  # "" the theme's own
//!     opacity = 0.75      # the surface's
//!     radius = 10         # a surface's corners, what is on it 4 less; unset, the theme's or 10
//!     density = "normal"  # compact, normal, comfortable: the control centre's rows; unset, the theme's
//!     language = "ru"     # its texts' language; unset, the locale's
//!     tab = "auto"        # a hovered block's and a tab's ground: wallpaper, bar; auto, the wallpaper with blur
//!     blur = true         # Hyprland's blur, its blur_size and blur_passes: set only when given here or by the theme
//!
//!     [widget.wallpaper]  # a control centre widget's own, as its schema says (settings/)
//!     dir = "~/Pictures/wallpapers"
//!
//!     [plugin.hello]      # a plugin's own settings (plugins/), handed to it, and again as they change
//!     greeting = "Hi"
//!
//! The Settings page (settings/) edits this file in place, its comments and order kept; secrets (a plugin's
//! password) go to the Secret Service, never here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default)]
pub struct Bar {
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
    /// the monitors with a bar (bars.rs)
    pub monitors: Monitors,
}

/// Which monitors have a bar: "all", "primary" (the one Hyprland has focused as ostrov starts, else the first), or
/// the ones named (["eDP-1", "DP-2"]), the first named the launcher's.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum Monitors {
    Which(Which),
    Named(Vec<String>),
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Which {
    All,
    Primary,
}

impl Default for Bar {
    fn default() -> Bar {
        let v = |s: &[&str]| s.iter().map(|s| s.to_string()).collect();
        Bar {
            left: v(&["workspaces", "window"]),
            center: v(&["panel.calendar"]),
            right: v(&["privacy", "layout", "tray", "panel.control"]),
            monitors: Monitors::Which(Which::All),
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Idle {
    pub lock: u32,
    pub screens_off: u32,
}

impl Default for Idle {
    fn default() -> Idle {
        Idle { lock: 600, screens_off: 900 }
    }
}

/// When the notifications keep quiet (their toasts back, the history kept): while a game has the focus (its window's
/// class one of games, a trailing * a prefix), between two times of day ("23:00", "08:00"; empty, never), and the
/// apps let through all the same. The games plugin's classes are a list of its own.
#[derive(Deserialize)]
#[serde(default)]
pub struct Notifications {
    pub quiet_in_games: bool,
    pub games: Vec<String>,
    pub quiet_from: String,
    pub quiet_to: String,
    pub allow: Vec<String>,
}

impl Default for Notifications {
    fn default() -> Notifications {
        Notifications {
            quiet_in_games: true,
            games: Vec::new(),
            quiet_from: String::new(),
            quiet_to: String::new(),
            allow: Vec::new(),
        }
    }
}

/// The launcher's: its web search's URL, {} the query escaped.
#[derive(Deserialize)]
#[serde(default)]
pub struct Launcher {
    pub search: String,
    /// more engines by their prefixes: "g " = "https://www.google.com/search?q={}", "?" = Claude's; what follows
    /// the prefix put in the URL's {} and opened, as s words is for the search above
    pub engines: BTreeMap<String, String>,
}

impl Default for Launcher {
    fn default() -> Launcher {
        Launcher { search: "https://duckduckgo.com/?q={}".into(), engines: BTreeMap::new() }
    }
}

/// The look: a theme's palette, the accent and the surface over it, the shapes, the density, the motion.
/// Hyprland's blur is set only from what is given, so a file without it leaves Hyprland's own.
#[derive(Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: String,
    pub accent: String,
    pub surface: String,
    pub opacity: f64,
    pub radius: Option<u32>,
    pub density: Option<String>,
    pub tab: String,
    pub animations: bool,
    pub blur: Option<bool>,
    pub blur_size: Option<u32>,
    pub blur_passes: Option<u32>,
    /// the text's family ("" GTK's own), its size in points, what the rest is sized from
    pub font: String,
    /// the language its texts are in ("" the locale's): en, ru
    pub language: String,
    pub font_size: f64,
    /// icons' size in pixels: on what opens, in the bar
    pub icon_size: u32,
    pub bar_icon_size: u32,
    /// the bar's height, a block's padding at its sides, the gap between a block's icons, in pixels
    pub bar_height: u32,
    pub bar_padding: u32,
    pub bar_spacing: u32,
    /// the bar's colour ("" the theme's) and how solid it is (unset: 0.65 over a wallpaper, solid over none)
    pub bar_color: String,
    pub bar_opacity: Option<f64>,
}

impl Default for Appearance {
    fn default() -> Appearance {
        Appearance {
            theme: "dark".into(),
            accent: String::new(),
            surface: String::new(),
            opacity: 0.75,
            radius: None,
            density: None,
            tab: "auto".into(),
            language: String::new(),
            animations: true,
            blur: None,
            blur_size: None,
            blur_passes: None,
            font: String::new(),
            font_size: 11.0,
            icon_size: 16,
            bar_icon_size: 14,
            bar_height: 25,
            bar_padding: 10,
            bar_spacing: 7,
            bar_color: String::new(),
            bar_opacity: None,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub bar: Bar,
    pub idle: Idle,
    pub colors: BTreeMap<String, String>,
    pub launcher: Launcher,
    pub notifications: Notifications,
    pub appearance: Appearance,
    /// the control centre widgets' own sections, [widget.ID]
    pub widget: BTreeMap<String, toml::Table>,
    pub plugin: BTreeMap<String, toml::Table>,
    /// panels of the user's own, [panels.ID], put in the bar as panel.ID; the control centre's and the
    /// calendar's name, icon or width changed the same way
    pub panels: BTreeMap<String, PanelSpec>,
    pub hyprland: Hyprland,
    pub polkit: Polkit,
    pub lock: LockCfg,
}

/// The lock screen's ground: black, or the screen as it was, blurred and darkened under the clock.
#[derive(Deserialize)]
#[serde(default)]
pub struct LockCfg {
    pub background: String,
}

impl Default for LockCfg {
    fn default() -> LockCfg {
        LockCfg { background: "black".into() }
    }
}

/// Whether ostrov is the session's polkit agent (polkit.rs): false leaves it to another (hyprpolkitagent, ...).
#[derive(Deserialize)]
#[serde(default)]
pub struct Polkit {
    pub agent: bool,
}

impl Default for Polkit {
    fn default() -> Polkit {
        Polkit { agent: true }
    }
}

/// What ostrov puts into Hyprland itself (modules/hyprland.rs): its layers' rules, its keys where free, a key
/// moved ([hyprland.keys] run = "SUPER, R").
#[derive(Deserialize)]
#[serde(default)]
pub struct Hyprland {
    pub rules: bool,
    pub binds: bool,
    pub keys: BTreeMap<String, String>,
}

impl Default for Hyprland {
    fn default() -> Hyprland {
        Hyprland { rules: true, binds: true, keys: BTreeMap::new() }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct PanelSpec {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub width: Option<i32>,
    /// its grid's width in cells (Edit's Width)
    pub cols: Option<u8>,
}

pub fn path() -> PathBuf {
    crate::hub::home().join(".config/ostrov/config.toml")
}

/// The config as the file says it now.
pub fn load() -> Config {
    let Ok(text) = std::fs::read_to_string(path()) else { return Config::default() };
    toml::from_str(&text).unwrap_or_else(|e| {
        eprintln!("ostrov: {}: {e}", path().display());
        Config::default()
    })
}

#[cfg(test)]
mod tests {
    /// config/example.toml, the documented defaults, reads as the config and says what the defaults say.
    #[test]
    fn example_is_the_defaults() {
        let c: super::Config = toml::from_str(include_str!("../config/example.toml")).expect("example.toml");
        let d = super::Config::default();
        assert_eq!((c.bar.left, c.bar.center, c.bar.right), (d.bar.left, d.bar.center, d.bar.right));
        assert_eq!(c.bar.monitors, d.bar.monitors);
        assert_eq!((c.idle.lock, c.idle.screens_off), (d.idle.lock, d.idle.screens_off));
        assert_eq!((c.appearance.theme, c.appearance.opacity, c.appearance.radius), (d.appearance.theme, 0.75, None));
        assert_eq!(c.notifications.games, d.notifications.games);
        assert_eq!(c.launcher.search, d.launcher.search);
        assert_eq!(c.lock.background, d.lock.background);
        assert!(c.hyprland.rules && c.hyprland.binds && c.hyprland.keys.is_empty() && c.panels.is_empty());
    }

    #[test]
    fn monitors_read() {
        use super::{Monitors, Which};
        let m = |t: &str| toml::from_str::<super::Bar>(t).map(|b| b.monitors).ok();
        assert_eq!(m(r#"monitors = "primary""#), Some(Monitors::Which(Which::Primary)));
        assert_eq!(m(r#"monitors = ["DP-2", "eDP-1"]"#), Some(Monitors::Named(vec!["DP-2".into(), "eDP-1".into()])));
        assert_eq!(m(""), Some(Monitors::Which(Which::All)));
        assert_eq!(m(r#"monitors = "some""#), None);
    }
}
