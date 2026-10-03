//! ostrov's config, ~/.config/ostrov/config.toml (dotfiles' config/ostrov): the bar's blocks, idle's times, the
//! palette's colours over style.rs's own. Every key may be left out for its default; a file that does not read is
//! said on stderr and the defaults taken. The colours follow the file as it changes; the rest is read at the start.
//!
//!     [bar]
//!     left = ["workspaces"]
//!     center = ["clock"]
//!     right = ["record", "privacy", "layout", "tray", "status"]
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
        Bar { left: v(&["workspaces"]), center: v(&["clock"]), right: v(&["record", "privacy", "layout", "tray", "status"]) }
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

/// The calendars: a CalDAV account (its password never here: a command prints it) and .ics links.
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

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub bar: Bar,
    pub idle: Idle,
    pub colors: BTreeMap<String, String>,
    pub calendar: Calendar,
    pub games: Games,
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
