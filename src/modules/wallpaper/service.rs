//! The wallpaper picked, ostrov's own: on or off (a plain ground), the picture, kept in
//! ~/.local/state/ostrov/wallpaper.json; the pictures to pick from in [widget.wallpaper]'s dir. A pick runs
//! [widget.wallpaper]'s on_change after it, the picture in $OSTROV_WALLPAPER ("" off): what else follows the
//! wallpaper (a terminal's see-through, a browser's colour) is the user's to say. Random is a pick of its own:
//! a picture at random now, and with [widget.wallpaper] interval, minutes, another once the one shown has been
//! there that long (tick); a picture picked by hand ends it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::services::Res;

#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct Pick {
    pub on: bool,
    pub path: String,
    /// picked as Random: the picture one at random, another every interval
    #[serde(default)]
    pub random: bool,
}

pub fn file() -> PathBuf {
    crate::hub::home().join(".local/state/ostrov/wallpaper.json")
}

/// The pick as it is kept; off without one.
pub fn pick() -> Pick {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// [widget.wallpaper]'s key, if set.
fn own(key: &str) -> Option<String> {
    crate::config::load().widget.get("wallpaper").and_then(|t| t.get(key)?.as_str().map(String::from)).filter(|s| !s.is_empty())
}

/// The pictures to pick from: the JPEGs and PNGs in the dir, sorted.
fn pictures() -> Vec<String> {
    let dir = own("dir").map_or_else(|| crate::hub::home().join("Pictures/wallpapers"), |d| crate::settings::expand(&d));
    let mut all: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|f| {
            let name = f.file_name().to_string_lossy().to_lowercase();
            name.rsplit_once('.').is_some_and(|(_, ext)| matches!(ext, "jpg" | "jpeg" | "png" | "webp"))
        })
        .map(|f| dir.join(f.file_name()).to_string_lossy().into_owned())
        .collect();
    all.sort();
    all
}

/// How solid the bar is: [appearance]'s bar_opacity, else 0.65 over a picture and solid over none.
pub fn bar_alpha() -> f64 {
    match crate::config::load().appearance.bar_opacity {
        Some(o) => o.clamp(0.0, 1.0),
        None if pick().on => 0.65,
        None => 1.0,
    }
}

pub fn state() -> Value {
    let p = pick();
    json!({"on": p.on, "path": p.path, "random": p.random, "wallpapers": pictures()})
}

/// The pick kept, and on_change run after it.
fn keep(p: Pick) -> Res {
    let f = file();
    if let Some(dir) = f.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&f, serde_json::to_string(&p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if let Some(hook) = own("on_change") {
        let shown = if p.on { p.path.as_str() } else { "" };
        match std::process::Command::new("sh").args(["-c", &hook]).env("OSTROV_WALLPAPER", shown).spawn() {
            // waited for, or it stays a zombie of ostrov's
            Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
            Err(e) => eprintln!("ostrov: wallpaper on_change: {e}"),
        }
    }
    Ok(())
}

/// Another picture than the one shown, at random, on, Random picked.
fn random(p: &Pick, all: &[String]) -> Result<Pick, String> {
    let others: Vec<&String> = all.iter().filter(|w| **w != p.path).collect();
    if others.is_empty() {
        return Err("no other picture".into());
    }
    let i = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as usize);
    Ok(Pick { on: true, path: others[i % others.len()].clone(), random: true })
}

/// Random picked, with [widget.wallpaper] interval: every so many minutes another picture at random, once the one
/// shown (its pick's file as old as that) has been there that long; 0 or unset, never. Called every minute.
pub fn tick() {
    let every = crate::config::load().widget.get("wallpaper").and_then(|t| t.get("interval")?.as_integer()).unwrap_or(0);
    let p = pick();
    if every <= 0 || !p.on || !p.random {
        return;
    }
    let age = std::fs::metadata(file()).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
    if age.is_some_and(|a| a.as_secs() >= every as u64 * 60)
        && let Err(e) = random(&p, &pictures()).and_then(keep)
    {
        eprintln!("ostrov: wallpaper interval: {e}");
    }
}

pub async fn cmd(args: &[&str]) -> Res {
    let mut p = pick();
    let all = pictures();
    match args {
        ["on"] => {
            p.on = true;
            if p.path.is_empty() {
                p.path = all.first().cloned().ok_or("no pictures to show")?;
            }
        }
        ["off"] => p.on = false,
        ["set", path] => {
            let path = crate::settings::expand(path);
            if !path.is_file() {
                return Err(format!("{} is no picture", path.display()));
            }
            p = Pick { on: true, path: path.to_string_lossy().into(), random: false };
        }
        ["random"] => p = random(&p, &all)?,
        _ => return Err(super::MODULE.usage()),
    }
    keep(p)
}
