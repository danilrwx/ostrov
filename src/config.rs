//! ostrov's config, ~/.config/ostrov/config.toml (dotfiles' config/ostrov): the bar's blocks, idle's times, the
//! palette's colours over style.rs's own. Every key may be left out for its default; a file that does not read is
//! said on stderr and the defaults taken. The colours follow the file as it changes; the rest is read at the start.
//!
//!     [bar]
//!     left = ["workspaces"]
//!     center = ["panel.calendar"]  # a panel: its widgets' badges, the panel unrolled out of them
//!     right = ["record", "privacy", "layout", "tray", "panel.control"]
//!
//!     [idle]
//!     lock = 600          # seconds idle to the lock, 0 never
//!     screens_off = 900   # to the screens off, 0 never
//!
//!     [colors]            # any of style.rs's palette
//!     surface = "rgba(0, 0, 0, 0.75)"
//!
//!     [calendar]          # the calendar popup's events (services/calendar.rs), read anew on every fetch
//!     caldav_url = "https://caldav.example.com"                     # any CalDAV server: Example, iCloud's
//!     user = "login@example.com"                                    # https://caldav.icloud.com, Fastmail's,
//!     password_command = "secret-tool lookup service caldav"      # Nextcloud's...; prints the app password
//!     ics = ["https://example.org/calendar.ics"]                  # and/or calendars' shared links
//!
//!     [games]             # their windows focused, the power profile theirs (services/games.rs)
//!     classes = ["dota2", "cs2", "steam_app_*"]
//!     profile = "performance"
//!
//!     [appearance]        # the look (style.rs), taken as the file is saved; the control centre's Appearance
//!     theme = "dark"      # dark, light, graphite, nord, solarized
//!     accent = "#5e81ac"  # "" the theme's own
//!     opacity = 0.75      # the surface's
//!     radius = 10         # a surface's corners, what is on it 4 less
//!     density = "normal"  # compact, normal, comfortable: the control centre's rows
//!     blur = true         # Hyprland's blur, its blur_size and blur_passes: set only when given here
//!
//!     [widget.wallpaper]  # a control centre widget's own, as its schema says (settings/)
//!     dir = "~/Pictures/wallpapers"
//!
//!     [plugin.hello]      # a plugin's own settings (plugins/), handed to it, and again as they change
//!     greeting = "Hi"
//!
//! The Settings page (settings/) edits this file in place, its comments and order kept; secrets (the calendar's
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
}

impl Default for Bar {
    fn default() -> Bar {
        let v = |s: &[&str]| s.iter().map(|s| s.to_string()).collect();
        Bar {
            left: v(&["workspaces", "window"]),
            center: v(&["panel.calendar"]),
            right: v(&["record", "privacy", "layout", "tray", "panel.control"]),
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

/// The games (window classes; a trailing * a prefix) played in a power profile of their own (services/games.rs).
#[derive(Deserialize)]
#[serde(default)]
pub struct Games {
    pub classes: Vec<String>,
    pub profile: String,
}

impl Default for Games {
    fn default() -> Games {
        Games { classes: ["dota2", "cs2", "steam_app_*"].map(String::from).to_vec(), profile: "performance".into() }
    }
}

/// The calendars: a CalDAV account (its password never here: in the Secret Service, else printed by a command)
/// and .ics links.
#[derive(Deserialize)]
#[serde(default)]
pub struct Calendar {
    pub caldav_url: String,
    pub user: String,
    pub password_command: String,
    pub ics: Vec<String>,
}

impl Default for Calendar {
    fn default() -> Calendar {
        Calendar {
            caldav_url: String::new(),
            user: String::new(),
            password_command: String::new(),
            ics: Vec::new(),
        }
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
    pub radius: u32,
    pub density: String,
    pub animations: bool,
    pub blur: Option<bool>,
    pub blur_size: Option<u32>,
    pub blur_passes: Option<u32>,
}

impl Default for Appearance {
    fn default() -> Appearance {
        Appearance {
            theme: "dark".into(),
            accent: String::new(),
            surface: String::new(),
            opacity: 0.75,
            radius: 10,
            density: "normal".into(),
            animations: true,
            blur: None,
            blur_size: None,
            blur_passes: None,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub bar: Bar,
    pub idle: Idle,
    pub colors: BTreeMap<String, String>,
    pub calendar: Calendar,
    pub games: Games,
    pub appearance: Appearance,
    /// the control centre widgets' own sections, [widget.ID]
    pub widget: BTreeMap<String, toml::Table>,
    pub plugin: BTreeMap<String, toml::Table>,
    /// panels of the user's own, [panels.ID], put in the bar as panel.ID; the control centre's and the
    /// calendar's name, icon or width changed the same way
    pub panels: BTreeMap<String, PanelSpec>,
    pub hyprland: Hyprland,
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
