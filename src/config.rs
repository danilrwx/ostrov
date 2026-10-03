//! ostrov's config, ~/.config/ostrov/config.toml (dotfiles' config/ostrov): the bar's blocks, idle's times, the
//! palette's colours over style.rs's own. Every key may be left out for its default; a file that does not read is
//! said on stderr and the defaults taken. The colours follow the file as it changes; the rest is read at the start.
//!
//!     [bar]
//!     left = ["workspaces"]
//!     center = ["clock"]
//!     right = ["layout", "tray", "status"]
//!
//!     [idle]
//!     lock = 600          # seconds idle to the lock, 0 never
//!     screens_off = 900   # to the screens off, 0 never
//!
//!     [colors]            # any of style.rs's palette
//!     surface = "rgba(0, 0, 0, 0.75)"

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
        Bar { left: v(&["workspaces"]), center: v(&["clock"]), right: v(&["layout", "tray", "status"]) }
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

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub bar: Bar,
    pub idle: Idle,
    pub colors: BTreeMap<String, String>,
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
