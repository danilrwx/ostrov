//! bin/theme's pick from its state files (theme.go).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// A state file's content, trimmed; "" without it.
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default().trim().into()
}

/// bin/theme's pick, as it keeps it in ~/.cache/theme: dark or dark-wall, the wallpaper, and the wallpapers to
/// pick from in ~/Pictures/wallpapers (or [widget.wallpaper]'s dir). bin/theme itself does the switching.
pub fn state() -> Value {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let state = match std::env::var("XDG_CACHE_HOME") {
        Ok(x) if !x.is_empty() => PathBuf::from(x).join("theme"),
        _ => home.join(".cache/theme"),
    };
    let mut mode = read(&state.join("mode"));
    if mode.is_empty() {
        mode = "dark".into();
    }
    // the wallpaper widget's directory (cc/widgets.rs's schema), ~ the home
    let own = crate::config::load().widget.get("wallpaper").and_then(|t| t.get("dir")?.as_str().map(String::from));
    let dir = match own {
        Some(d) if !d.is_empty() => crate::settings::expand(&d),
        _ => home.join("Pictures/wallpapers"),
    };
    let mut wallpapers: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|f| {
            let name = f.file_name().to_string_lossy().to_lowercase();
            name.rsplit_once('.').is_some_and(|(_, ext)| matches!(ext, "jpg" | "jpeg" | "png"))
        })
        .map(|f| dir.join(f.file_name()).to_string_lossy().into_owned())
        .collect();
    wallpapers.sort();
    json!({"mode": mode, "wallpaper": read(&state.join("wallpaper")), "wallpapers": wallpapers})
}

#[cfg(test)]
mod tests {
    #[test]
    fn theme_state() {
        println!("theme {}", super::state());
    }
}
