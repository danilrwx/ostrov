//! Themes as packages: a directory with a theme.toml (its name, author, version, colours for style.rs's palette
//! after dark and, in [light], by day; suggested radius, density and blur) and an optional theme.css laid after
//! ostrov's own. The built-in five are the same files, under themes/ in the source, compiled in; installed ones
//! live in ~/.local/share/ostrov/themes/<id>/, the id the directory's name, and one of a built-in's id stands in for
//! it. [appearance] theme picks one and mode its dark or light; `ostrov theme list|set|install|remove` manages them
//! (docs/themes.md).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gtk4::gio;
use serde::Deserialize;

/// The built-in themes, in the order the Appearance page shows them.
const BUILT_IN: &[(&str, &str)] = &[
    ("ostrov", include_str!("../themes/ostrov/theme.toml")),
    ("graphite", include_str!("../themes/graphite/theme.toml")),
    ("nord", include_str!("../themes/nord/theme.toml")),
    ("solarized", include_str!("../themes/solarized/theme.toml")),
    ("catppuccin", include_str!("../themes/catppuccin/theme.toml")),
];

/// The ids themes had before each came in two: ostrov's, after dark or by day.
const ALIASES: &[(&str, &str, bool)] = &[("dark", "ostrov", false), ("light", "ostrov", true)];

fn yes() -> bool {
    true
}

#[derive(Deserialize, Debug, Default)]
pub struct Theme {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub version: String,
    /// Whether its [colors] are for after dark (a theme of one kind, light, says false): GTK's own widgets (the
    /// colour dialog) follow it; after variant, whether the colours taken are.
    #[serde(default = "yes")]
    pub dark: bool,
    /// style.rs's tokens it sets; surface's colour taken under [appearance]'s opacity.
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    /// The same by day, if it has a light side.
    #[serde(default)]
    pub light: BTreeMap<String, String>,
    /// Suggestions, taken while [appearance] says nothing of them.
    pub radius: Option<u32>,
    pub density: Option<String>,
    pub blur: Option<bool>,
    /// theme.css, laid after ostrov's CSS.
    #[serde(skip)]
    pub css: String,
    #[serde(skip)]
    pub built_in: bool,
}

/// Whether s does as an id: of [a-z0-9-], since it goes into CSS classes and the command line.
fn word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A theme.toml read and checked: its colours style.rs's tokens with values that keep the CSS whole, a density
/// ostrov has, a radius the Appearance page's slider has.
pub fn parse(text: &str, id: &str, css: String) -> Result<Theme, String> {
    if !word(id) {
        return Err(format!("id {id:?}: not of [a-z0-9-]"));
    }
    let mut t: Theme = toml::from_str(text).map_err(|e| e.to_string())?;
    if t.name.trim().is_empty() {
        return Err("no name".into());
    }
    let tokens = crate::style::tokens();
    for (table, colors) in [("colors", &t.colors), ("light", &t.light)] {
        for (k, v) in colors {
            if !tokens.contains(&k.as_str()) {
                return Err(format!("{table}.{k}: not one of ostrov's tokens ({})", tokens.join(", ")));
            }
            if v.trim().is_empty() || v.contains([';', '{', '}']) {
                return Err(format!("{table}.{k}: {v:?} is not a colour"));
            }
        }
    }
    if let Some(d) = t.density.as_deref().filter(|d| !["compact", "normal", "comfortable"].contains(d)) {
        return Err(format!("density {d:?}: compact, normal or comfortable"));
    }
    if t.radius.is_some_and(|r| r > 20) {
        return Err("radius: 0 to 20".into());
    }
    t.id = id.into();
    t.css = css;
    Ok(t)
}

/// Where installed themes live.
pub fn dir() -> PathBuf {
    crate::hub::home().join(".local/share/ostrov/themes")
}

/// A theme's directory read: theme.toml, theme.css if there is one, the id its name.
fn read(path: &Path, id: &str) -> Result<Theme, String> {
    let text = std::fs::read_to_string(path.join("theme.toml")).map_err(|e| format!("theme.toml: {e}"))?;
    parse(&text, id, std::fs::read_to_string(path.join("theme.css")).unwrap_or_default())
}

fn built_in() -> Vec<Theme> {
    let parsed = BUILT_IN.iter().filter_map(|(id, text)| parse(text, id, String::new()).ok());
    parsed.map(|t| Theme { built_in: true, ..t }).collect()
}

/// The installed themes' directories by id, read or not, in order (a directory named with a dot, an install's
/// staging, left out).
fn installed(root: &Path) -> Vec<(String, Result<Theme, String>)> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<_> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|id| !id.starts_with('.'))
        .map(|id| {
            let t = read(&root.join(&id), &id);
            (id, t)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Every theme that reads, the built-in first (an installed one of the same id in its place), then the installed.
pub fn all() -> Vec<Theme> {
    let mut out = built_in();
    for t in installed(&dir()).into_iter().filter_map(|(_, t)| t.ok()) {
        match out.iter_mut().find(|b| b.id == t.id) {
            Some(b) => *b = t,
            None => out.push(t),
        }
    }
    out
}

impl Theme {
    /// Whether it has both sides.
    pub fn two(&self) -> bool {
        self.dark && !self.light.is_empty()
    }

    /// Its side for the mode: by day its [light] over the others, after dark its [colors]; a theme of one side its
    /// one whatever the mode.
    pub fn variant(mut self, light: bool) -> Theme {
        if light && self.two() {
            self.colors = std::mem::take(&mut self.light);
            self.dark = false;
        }
        self
    }
}

/// An id as it is now: an old one (dark, light) ostrov's.
pub fn canonical(id: &str) -> &str {
    ALIASES.iter().find(|(old, ..)| *old == id).map_or(id, |a| a.1)
}

/// Every theme on the mode's side (the Appearance page's cards).
pub fn all_now() -> Vec<Theme> {
    let light = light_mode();
    all().into_iter().map(|t| t.variant(light)).collect()
}

/// Whether ostrov is by day: [appearance] mode, auto as [appearance] day says now, else (left out) as an old
/// theme id said, "light".
pub fn light_mode() -> bool {
    let a = crate::config::load().appearance;
    match a.mode.as_str() {
        "light" => true,
        "dark" => false,
        "auto" => day_now(&a.day),
        _ => ALIASES.iter().any(|(old, _, light)| *old == a.theme && *light),
    }
}

/// Whether it is day now as day says: "HH:MM-HH:MM" (local), or "sun", sunrise to sunset where `ostrov location`
/// put the machine (with none, 07:00 to 19:00).
fn day_now(day: &str) -> bool {
    let Ok(now) = gtk4::glib::DateTime::now_local() else { return true };
    let minute = now.hour() * 60 + now.minute();
    if day.trim() == "sun" {
        if let Some(l) = crate::services::location::load() {
            let utc = now.to_utc().unwrap_or(now.clone());
            return sun_up(utc.day_of_year(), utc.hour() as f64 * 60.0 + utc.minute() as f64, l.lat, l.lon);
        }
        return (7 * 60..19 * 60).contains(&minute);
    }
    let clock = |s: &str| {
        let (h, m) = s.trim().split_once(':')?;
        Some(h.parse::<i32>().ok()? * 60 + m.parse::<i32>().ok()?)
    };
    match day.split_once('-').and_then(|(a, b)| Some((clock(a)?, clock(b)?))) {
        Some((from, to)) if from <= to => (from..to).contains(&minute),
        Some((from, to)) => minute >= from || minute < to,
        None => (7 * 60..19 * 60).contains(&minute),
    }
}

/// Whether the sun is up on that day of the year at that minute of the day (UTC), at lat, lon: NOAA's simplified
/// equations (a minute or two off); a polar day up, a polar night down.
fn sun_up(day_of_year: i32, minute_utc: f64, lat: f64, lon: f64) -> bool {
    use std::f64::consts::PI;
    let rad = PI / 180.0;
    let g = 2.0 * PI / 365.0 * (day_of_year as f64 - 1.0);
    let eqtime = 229.18
        * (0.000075 + 0.001868 * g.cos() - 0.032077 * g.sin() - 0.014615 * (2.0 * g).cos() - 0.040849 * (2.0 * g).sin());
    let decl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos() + 0.000907 * (2.0 * g).sin()
        - 0.002697 * (3.0 * g).cos()
        + 0.00148 * (3.0 * g).sin();
    let cos_h = (90.833 * rad).cos() / ((lat * rad).cos() * decl.cos()) - (lat * rad).tan() * decl.tan();
    if cos_h < -1.0 {
        return true;
    }
    if cos_h > 1.0 {
        return false;
    }
    let ha = cos_h.acos() / rad;
    let (rise, set) = (720.0 - 4.0 * (lon + ha) - eqtime, 720.0 - 4.0 * (lon - ha) - eqtime);
    // the day's minutes past midnight, the sun's hours maybe across it
    [minute_utc - 1440.0, minute_utc, minute_utc + 1440.0].iter().any(|m| (rise..set).contains(m))
}

/// The theme of that id on the mode's side; ostrov's for an id there is none of (or one that does not read), an
/// old id (dark, light) ostrov's too.
pub fn get(id: &str) -> Theme {
    let id = canonical(id);
    let mut all = all();
    let t = match all.iter().position(|t| t.id == id).or(all.iter().position(|t| t.id == "ostrov")) {
        Some(i) => all.swap_remove(i),
        None => Theme::default(),
    };
    t.variant(light_mode())
}

/// `ostrov theme ARGS`, the running ostrov's.
pub async fn command(args: Vec<String>) -> Result<String, String> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args[..] {
        ["list"] => Ok(list()),
        ["set", id] => {
            if !all().iter().any(|t| t.id == id) {
                return Err(format!("no theme {id} (ostrov theme list)"));
            }
            crate::settings::write("appearance", "theme", Some(&id.into()), false)?;
            crate::style::reload();
            Ok(String::new())
        }
        ["install", src] => {
            let out = install(src, &dir()).await?;
            crate::style::reload();
            Ok(out)
        }
        ["remove", id] => {
            let path = dir().join(id);
            if !word(id) || !path.is_dir() {
                let why = if built_in().iter().any(|t| t.id == id) { "built in" } else { "not installed" };
                return Err(format!("{id}: {why}"));
            }
            std::fs::remove_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            crate::style::reload();
            Ok(String::new())
        }
        _ => {
            let forms: Vec<&str> = crate::FORMS.iter().filter_map(|f| f.strip_prefix("theme ")).collect();
            Err(crate::forms::usage("theme", &forms))
        }
    }
}

/// A line per theme: the id, the name, by whom, built in or installed, the one picked starred; the installed
/// ones that do not read with why.
fn list() -> String {
    let now = crate::config::load().appearance.theme;
    let mut lines: Vec<String> = all()
        .iter()
        .map(|t| {
            let star = if t.id == now { "*" } else { " " };
            let by = [t.author.as_str(), t.version.as_str()].join(" ");
            let from = if t.built_in { "built in" } else { "installed" };
            format!("{star} {}\t{}\t{}\t{from}", t.id, t.name, by.trim())
        })
        .collect();
    for (id, e) in installed(&dir()).into_iter().filter_map(|(id, t)| t.err().map(|e| (id, e))) {
        lines.push(format!("! {id}\t{e}"));
    }
    lines.join("\n")
}

/// Whether src is a git repository's URL rather than a path.
fn is_url(src: &str) -> bool {
    src.contains("://") || src.starts_with("git@")
}

/// A theme installed from a directory or a git repository's URL: staged in the themes' directory, read, then put
/// in place of one of its id (an upgrade). Its id the directory's name or the repository's, less .git and an
/// "ostrov-theme-" in front.
async fn install(src: &str, root: &Path) -> Result<String, String> {
    std::fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let stage = root.join(format!(".install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&stage);
    let name = if is_url(src) {
        clone(src, &stage).await?;
        let _ = std::fs::remove_dir_all(stage.join(".git"));
        repo_name(src)
    } else {
        let mut path = crate::settings::expand(src);
        if path.ends_with("theme.toml") {
            path.pop();
        }
        let path = path.canonicalize().map_err(|e| format!("{src}: {e}"))?;
        copy(&path, &stage).map_err(|e| format!("{src}: {e}"))?;
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    };
    let id = name.strip_prefix("ostrov-theme-").unwrap_or(&name).to_string();
    let theme = match read(&stage, &id) {
        Ok(t) => t,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&stage);
            return Err(format!("{src}: {e}"));
        }
    };
    let to = root.join(&id);
    let _ = std::fs::remove_dir_all(&to);
    std::fs::rename(&stage, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    Ok(format!("{id}: {} installed (ostrov theme set {id})", theme.name))
}

/// A repository's name from its URL: its last part, less a /.git or .git after it.
fn repo_name(url: &str) -> String {
    let url = url.trim_end_matches('/');
    let url = url.strip_suffix("/.git").or(url.strip_suffix(".git")).unwrap_or(url);
    url.rsplit(['/', ':']).next().unwrap_or("").to_string()
}

/// git clone, shallow, asking nothing (no terminal to ask in).
async fn clone(url: &str, to: &Path) -> Result<(), String> {
    let flags = gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_PIPE;
    let launcher = gio::SubprocessLauncher::new(flags);
    launcher.setenv("GIT_TERMINAL_PROMPT", "0", true);
    let to = to.to_string_lossy();
    let argv = ["git", "clone", "--depth", "1", "--", url, to.as_ref()].map(std::ffi::OsStr::new);
    let p = launcher.spawn(&argv).map_err(|e| format!("git: {e}"))?;
    let (_, err) = p.communicate_utf8_future(None).await.map_err(|e| format!("git: {e}"))?;
    if !p.is_successful() {
        let _ = std::fs::remove_dir_all(to.as_ref());
        let err = err.unwrap_or_default();
        return Err(format!("git clone {url}: {}", err.trim().lines().last().unwrap_or("failed")));
    }
    Ok(())
}

/// A directory copied whole, but for .git.
fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            if e.file_name() != ".git" {
                copy(&e.path(), &target)?;
            }
        } else {
            std::fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_read() {
        let b = built_in();
        let ids: Vec<_> = b.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["ostrov", "graphite", "nord", "solarized", "catppuccin"]);
        // each in two, both sides with a surface
        assert!(b.iter().all(|t| t.two() && t.colors.contains_key("surface") && t.light.contains_key("surface")));
        let nord = b.into_iter().find(|t| t.id == "nord").unwrap().variant(true);
        assert_eq!((nord.dark, nord.colors["surface"].as_str()), (false, "#eceff4"));
        let ex = include_str!("../examples/themes/catppuccin-mocha/theme.toml");
        let css = include_str!("../examples/themes/catppuccin-mocha/theme.css");
        assert_eq!(parse(ex, "catppuccin-mocha", css.into()).map(|t| t.name), Ok("Catppuccin Mocha".into()));
    }

    #[test]
    fn day_and_night() {
        // Riga at the equinox: up about 04:20 to 16:30 UTC
        assert!(sun_up(80, 12.0 * 60.0, 56.95, 24.1));
        assert!(!sun_up(80, 2.0 * 60.0, 56.95, 24.1));
        assert!(!sun_up(80, 20.0 * 60.0, 56.95, 24.1));
        // the far north in June: up at midnight
        assert!(sun_up(172, 0.0, 78.0, 15.0));
    }

    #[test]
    fn theme_toml() {
        let t = parse(
            "name = \"Paper\"\nauthor = \"me\"\nversion = \"2\"\ndark = false\nradius = 4\ndensity = \"compact\"\n\
             blur = false\n[colors]\nfg = \"#111\"\nsurface = \"rgb(250, 250, 250)\"\n",
            "paper",
            "label {}".into(),
        )
        .unwrap();
        assert_eq!((t.id.as_str(), t.name.as_str()), ("paper", "Paper"));
        assert_eq!((t.author.as_str(), t.version.as_str()), ("me", "2"));
        assert_eq!((t.dark, t.radius, t.density.as_deref(), t.blur), (false, Some(4), Some("compact"), Some(false)));
        assert_eq!((t.colors["fg"].as_str(), t.css.as_str()), ("#111", "label {}"));
        let bad = |text: &str, id: &str| parse(text, id, String::new()).unwrap_err();
        assert!(bad("name = \"A\"\ndark = true\n", "Has Space").contains("[a-z0-9-]"));
        assert!(bad("name = \"A\"\n[light]\nfgg = \"#fff\"\n", "a").contains("light.fgg"));
        assert!(bad("name = \"\"\ndark = true\n", "a").contains("no name"));
        assert!(bad("name = \"A\"\ndark = true\n[colors]\nfgg = \"#fff\"\n", "a").contains("colors.fgg"));
        assert!(bad("name = \"A\"\ndark = true\n[colors]\nfg = \"red; }\"\n", "a").contains("not a colour"));
        assert!(bad("name = \"A\"\ndark = true\ndensity = \"huge\"\n", "a").contains("density"));
        assert!(bad("name = \"A\"\ndark = true\nradius = 40\n", "a").contains("radius"));
    }

    #[test]
    fn repo_names() {
        assert_eq!(repo_name("https://example.com/me/ostrov-theme-paper.git"), "ostrov-theme-paper");
        assert_eq!(repo_name("git@example.com:me/paper"), "paper");
        assert_eq!(repo_name("file:///src/paper/.git"), "paper");
        assert_eq!(repo_name("https://example.com/me/paper/"), "paper");
        assert!(is_url("git@example.com:me/paper") && is_url("file:///x") && !is_url("~/themes/paper"));
    }

    #[test]
    fn installed_from_a_directory() {
        let home = std::env::temp_dir().join(format!("ostrov-theme-test-{}", std::process::id()));
        let src = home.join("src/ostrov-theme-paper");
        std::fs::create_dir_all(src.join(".git")).unwrap();
        std::fs::write(src.join("theme.toml"), "name = \"Paper\"\ndark = false\n").unwrap();
        std::fs::write(src.join("theme.css"), ".dot { }").unwrap();
        std::fs::write(src.join(".git/HEAD"), "x").unwrap();
        let root = home.join("themes");
        let install = |src: &Path| gtk4::glib::MainContext::default().block_on(install(&src.to_string_lossy(), &root));
        assert_eq!(install(&src), Ok("paper: Paper installed (ostrov theme set paper)".into()));
        assert!(!root.join("paper/.git").exists());
        let t = read(&root.join("paper"), "paper").unwrap();
        assert_eq!((t.name.as_str(), t.css.as_str(), t.built_in), ("Paper", ".dot { }", false));
        // a theme that does not read leaves the installed one as it was, and no staging behind
        std::fs::write(src.join("theme.toml"), "name = \"\"\n").unwrap();
        assert!(install(&src.join("theme.toml")).unwrap_err().contains("no name"));
        let all = installed(&root);
        assert_eq!(all.iter().map(|(id, t)| (id.as_str(), t.is_ok())).collect::<Vec<_>>(), [("paper", true)]);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }
}
