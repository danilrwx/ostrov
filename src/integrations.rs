//! The apps ostrov colours by a file: each an app.toml and a template (apps/<id>/ in the source, compiled in; the
//! user's own in ~/.config/ostrov/apps/<id>/, one of a built-in's id standing in for it). Its template is written,
//! the palette's {{tokens}} filled in (apps.rs), to ~/.local/state/ostrov/colors/ whenever the look changes, for
//! every app there is on this machine; the app's config made to include that file only once the user connects it
//! (`ostrov apps connect ID`, the Appearance page's Apps), and the line taken out again as it is disconnected
//! ([apps] connected). An app told to read it again as it changes, its way (a signal, a command). Plugins bring
//! theirs in their manifests' [[apps]], their ids <plugin>-<id>. docs/apps.md.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::apps::Palette;

/// The built-in integrations: the id, app.toml, the template's name and text.
const BUILT_IN: &[(&str, &str, &str, &str)] = &[
    ("alacritty", include_str!("../apps/alacritty/app.toml"), "alacritty.toml", include_str!("../apps/alacritty/alacritty.toml")),
    ("kitty", include_str!("../apps/kitty/app.toml"), "kitty.conf", include_str!("../apps/kitty/kitty.conf")),
    ("foot", include_str!("../apps/foot/app.toml"), "foot.ini", include_str!("../apps/foot/foot.ini")),
    ("ghostty", include_str!("../apps/ghostty/app.toml"), "ghostty", include_str!("../apps/ghostty/ghostty")),
    ("k9s", include_str!("../apps/k9s/app.toml"), "k9s.yaml", include_str!("../apps/k9s/k9s.yaml")),
    ("telegram", include_str!("../apps/telegram/app.toml"), "telegram.tdesktop-palette", include_str!("../apps/telegram/telegram.tdesktop-palette")),
    ("shell", include_str!("../apps/shell/app.toml"), "colors.sh", include_str!("../apps/shell/colors.sh")),
    ("wezterm", include_str!("../apps/wezterm/app.toml"), "ostrov-wezterm.toml", include_str!("../apps/wezterm/ostrov-wezterm.toml")),
    ("btop", include_str!("../apps/btop/app.toml"), "ostrov.theme", include_str!("../apps/btop/ostrov.theme")),
    ("tmux", include_str!("../apps/tmux/app.toml"), "tmux.conf", include_str!("../apps/tmux/tmux.conf")),
    ("nvim", include_str!("../apps/nvim/app.toml"), "ostrov.lua", include_str!("../apps/nvim/ostrov.lua")),
    ("helix", include_str!("../apps/helix/app.toml"), "ostrov-helix.toml", include_str!("../apps/helix/ostrov-helix.toml")),
    ("vesktop", include_str!("../apps/vesktop/app.toml"), "ostrov.theme.css", include_str!("../apps/vesktop/ostrov.theme.css")),
    ("hyprland", include_str!("../apps/hyprland/app.toml"), "hyprland.conf", include_str!("../apps/hyprland/hyprland.conf")),
    ("zathura", include_str!("../apps/zathura/app.toml"), "zathurarc", include_str!("../apps/zathura/zathurarc")),
    ("fzf", include_str!("../apps/fzf/app.toml"), "fzf.sh", include_str!("../apps/fzf/fzf.sh")),
];

/// How an app's config is made to read ostrov's file.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Include {
    /// the config, ~ the home; alternatives split by | (the first there, else the first)
    #[serde(default)]
    pub file: String,
    /// a line put in it: at its end, or (section) right under that section's header
    #[serde(default)]
    pub line: String,
    #[serde(default)]
    pub section: String,
    /// a TOML array of paths (alacritty's general.import) the file's path put first in
    #[serde(default)]
    pub key: String,
    /// a link made to the file instead (k9s's skins/ostrov.yaml)
    #[serde(default)]
    pub link: String,
}

/// How an app is told to read it again: a signal to its processes by name, or a command.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Reload {
    #[serde(default)]
    pub signal: String,
    #[serde(default)]
    pub process: String,
    #[serde(default)]
    pub command: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct App {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    /// commands one of which in PATH, or paths (~ the home) one of which there, says the app is on this machine;
    /// none: always
    #[serde(default)]
    pub detect: Vec<String>,
    /// the template's file name, the name written under colors/ too
    pub template: String,
    #[serde(skip)]
    pub text: String,
    pub include: Option<Include>,
    pub reload: Option<Reload>,
    #[serde(skip)]
    pub built_in: bool,
}

fn home() -> PathBuf {
    crate::hub::home()
}

/// ~ the home.
fn expand(p: &str) -> PathBuf {
    crate::settings::expand(p)
}

/// An include's config: of its alternatives (a|b) the first there, else the first.
fn config(file: &str) -> PathBuf {
    let all: Vec<PathBuf> = file.split('|').map(|f| expand(f.trim())).collect();
    all.iter().find(|p| p.exists()).cloned().unwrap_or_else(|| all[0].clone())
}

/// Where the colour files go.
pub fn out() -> PathBuf {
    home().join(".local/state/ostrov/colors")
}

fn parse(id: &str, toml_text: &str, text: &str) -> Result<App, String> {
    let mut a: App = toml::from_str(toml_text).map_err(|e| format!("{id}: {e}"))?;
    if a.template.contains('/') || a.template.is_empty() {
        return Err(format!("{id}: template is a file name beside app.toml"));
    }
    a.id = id.into();
    a.text = text.into();
    Ok(a)
}

/// Every integration that reads: the built-in, the user's over one of the same id, then the user's others.
pub fn all() -> Vec<App> {
    let mut out: Vec<App> = BUILT_IN
        .iter()
        .filter_map(|(id, toml_text, _, text)| parse(id, toml_text, text).ok())
        .map(|a| App { built_in: true, ..a })
        .collect();
    let dir = home().join(".config/ostrov/apps");
    let mut own: Vec<_> = std::fs::read_dir(&dir).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).collect();
    own.sort_by_key(|e| e.file_name());
    // the plugins' running, by their ids: <plugin>-<id>
    for (plugin, dir, table) in crate::plugins::integrations() {
        let id = format!("{plugin}-{}", table.get("id").and_then(|v| v.as_str()).unwrap_or("app"));
        let mut t = table.clone();
        t.remove("id");
        let read = toml::to_string(&t).map_err(|e| e.to_string()).and_then(|text| {
            let a: App = toml::from_str(&text).map_err(|e| format!("{id}: {e}"))?;
            let tmpl = std::fs::read_to_string(dir.join(&a.template)).map_err(|e| format!("{id}: {}: {e}", a.template))?;
            parse(&id, &text, &tmpl)
        });
        match read {
            Ok(a) if !out.iter().any(|b| b.id == a.id) => out.push(a),
            Ok(_) => {}
            Err(e) => eprintln!("ostrov: apps: {e}"),
        }
    }
    for e in own {
        let id = e.file_name().to_string_lossy().into_owned();
        let read = std::fs::read_to_string(e.path().join("app.toml")).map_err(|e| format!("{id}: app.toml: {e}")).and_then(|t| {
            let a: App = toml::from_str(&t).map_err(|e| format!("{id}: {e}"))?;
            let text = std::fs::read_to_string(e.path().join(&a.template)).map_err(|e| format!("{id}: {}: {e}", a.template))?;
            parse(&id, &t, &text)
        });
        match read {
            Ok(a) => match out.iter_mut().find(|b| b.id == a.id) {
                Some(b) => *b = a,
                None => out.push(a),
            },
            Err(e) => eprintln!("ostrov: apps: {e}"),
        }
    }
    out
}

impl App {
    /// Whether the app is on this machine.
    pub fn here(&self) -> bool {
        self.detect.is_empty()
            || self.detect.iter().any(|d| if d.contains('/') { expand(d).exists() } else { in_path(d) })
    }

    /// The file written for it.
    pub fn file(&self) -> PathBuf {
        out().join(&self.template)
    }

    /// Its file as the configs name it: ~ for the home.
    fn named(&self) -> String {
        format!("~/.local/state/ostrov/colors/{}", self.template)
    }

    /// Whether its config reads ostrov's file now (said by the config itself, whoever put it there).
    pub fn connected(&self) -> bool {
        let Some(inc) = &self.include else { return false };
        if !inc.link.is_empty() {
            return std::fs::read_link(expand(&inc.link)).is_ok_and(|t| t == self.file());
        }
        let text = std::fs::read_to_string(config(&inc.file)).unwrap_or_default();
        text.contains(&self.named()) || text.contains(&self.file().to_string_lossy().into_owned())
    }

    /// Its config made to read ostrov's file.
    pub fn connect(&self) -> Result<(), String> {
        let Some(inc) = &self.include else { return Err(format!("{}: nothing to connect, its file is to open", self.name)) };
        if self.connected() {
            return Ok(());
        }
        if !inc.link.is_empty() {
            let link = expand(&inc.link);
            if let Some(d) = link.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            return std::os::unix::fs::symlink(self.file(), &link).map_err(|e| format!("{}: {e}", link.display()));
        }
        let path = config(&inc.file);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        write_through(&path, &with(&text, inc, &self.named()))
    }

    /// The line (or link) taken out again.
    pub fn disconnect(&self) -> Result<(), String> {
        let Some(inc) = &self.include else { return Ok(()) };
        if !inc.link.is_empty() {
            let link = expand(&inc.link);
            return if self.connected() { std::fs::remove_file(&link).map_err(|e| e.to_string()) } else { Ok(()) };
        }
        let path = config(&inc.file);
        let Ok(text) = std::fs::read_to_string(&path) else { return Ok(()) };
        let now = without(&text, &self.named());
        if now != text { write_through(&path, &now) } else { Ok(()) }
    }

    /// Told to read its file again (its command with the palette's tokens filled in).
    fn reload(&self, p: &Palette) {
        let Some(r) = &self.reload else { return };
        if !r.signal.is_empty() && !r.process.is_empty() {
            let _ = std::process::Command::new("pkill").args([&format!("-{}", r.signal), "-x", &r.process]).status();
        }
        if !r.command.is_empty() {
            let _ = std::process::Command::new("sh").args(["-c", &p.fill(&r.command)]).spawn();
        }
    }
}

fn in_path(cmd: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
}

/// A config written as it is: through a link to where it points (dotfiles' links kept links).
fn write_through(path: &Path, text: &str) -> Result<(), String> {
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(d) = real.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&real, text).map_err(|e| format!("{}: {e}", real.display()))
}

/// The config with ostrov's file included, as inc says.
fn with(text: &str, inc: &Include, named: &str) -> String {
    if !inc.key.is_empty() {
        // a TOML array of paths: the file first in it, the key made if there is none
        let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else { return text.into() };
        let (table, key) = inc.key.rsplit_once('.').unwrap_or(("", &inc.key));
        let mut t = doc.as_table_mut();
        for part in table.split('.').filter(|p| !p.is_empty()) {
            let item = t.entry(part).or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
            let Some(next) = item.as_table_mut() else { return text.into() };
            t = next;
        }
        let item = t.entry(key).or_insert(toml_edit::value(toml_edit::Array::new()));
        if let Some(arr) = item.as_array_mut() {
            arr.insert(0, named);
            arr.fmt();
        }
        return doc.to_string();
    }
    let line = inc.line.trim();
    if !inc.section.is_empty() {
        // right under its section's header, the header made at the top if there is none
        let mut out = String::new();
        let mut done = false;
        for l in text.lines() {
            out += l;
            out += "\n";
            if !done && l.trim() == inc.section {
                out += line;
                out += "\n";
                done = true;
            }
        }
        return if done { out } else { format!("{}\n{line}\n{text}", inc.section) };
    }
    let sep = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
    format!("{text}{sep}{line}\n")
}

/// The config without ostrov's file: its line, or its entry in an array.
fn without(text: &str, named: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for l in text.lines() {
        if !l.contains(named) {
            out.push(l.into());
            continue;
        }
        // in an array beside others: only its entry
        let quoted = format!("\"{named}\"");
        let rest = l.replace(&format!("{quoted}, "), "").replace(&format!(", {quoted}"), "");
        if rest.contains('[') && !rest.contains(named) {
            out.push(rest);
        }
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Every app's file written as the palette has it, those connected told to read it again as it changed.
pub fn apply(p: &Palette) {
    let connected = crate::config::load().apps.connected;
    for a in all().into_iter().filter(App::here) {
        if put(&a.file(), &p.fill(&a.text)) && (connected.contains(&a.id) || a.connected()) {
            a.reload(p);
        }
    }
}

fn put(path: &Path, text: &str) -> bool {
    if std::fs::read_to_string(path).ok().as_deref() == Some(text) {
        return false;
    }
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(path, text).is_ok()
}

/// [apps] connected with id in or out.
fn remember(id: &str, on: bool) -> Result<(), String> {
    let mut list = crate::config::load().apps.connected;
    list.retain(|x| x != id);
    if on {
        list.push(id.into());
    }
    let v = serde_json::Value::Array(list.into_iter().map(serde_json::Value::String).collect());
    crate::settings::write("apps", "connected", Some(&v), false)
}

/// Connected or not, its config and [apps] connected both.
pub fn set(id: &str, on: bool) -> Result<(), String> {
    let a = all().into_iter().find(|a| a.id == id).ok_or(format!("no app {id} (ostrov apps)"))?;
    if on { a.connect()? } else { a.disconnect()? }
    remember(id, on)
}

/// `ostrov apps [list] | connect ID | disconnect ID | apply`.
pub fn command(args: &[&str]) -> Result<String, String> {
    match args {
        [] | ["list"] => Ok(all()
            .iter()
            .map(|a| {
                let state = if !a.here() {
                    "not here"
                } else if a.include.is_none() {
                    "a file to open"
                } else if a.connected() {
                    "connected"
                } else {
                    "not connected"
                };
                let from = if a.built_in { "built in" } else { "yours" };
                format!("{}\t{}\t{state}\t{from}\t{}", a.id, a.name, a.file().display())
            })
            .collect::<Vec<_>>()
            .join("\n")),
        ["connect", id] => set(id, true).map(|()| String::new()),
        ["disconnect", id] => set(id, false).map(|()| String::new()),
        ["apply"] => {
            crate::style::reload();
            Ok(String::new())
        }
        _ => Err("usage: ostrov apps [list] | connect ID | disconnect ID | apply".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: &str = "~/.local/state/ostrov/colors/x";

    fn inc(line: &str, section: &str, key: &str) -> Include {
        Include { line: line.into(), section: section.into(), key: key.into(), ..Include::default() }
    }

    #[test]
    fn built_in_read() {
        let ids: Vec<_> = all().into_iter().filter(|a| a.built_in).map(|a| a.id).collect();
        assert_eq!(ids, [
            "alacritty", "kitty", "foot", "ghostty", "k9s", "telegram", "shell", "wezterm", "btop", "tmux", "nvim", "helix",
            "vesktop", "hyprland", "zathura", "fzf",
        ]);
        // each its own file
        let mut files: Vec<_> = all().into_iter().map(|a| a.template).collect();
        let n = files.len();
        files.sort();
        files.dedup();
        assert_eq!(files.len(), n);
    }

    #[test]
    fn a_line_in_and_out() {
        let i = inc("include ~/.local/state/ostrov/colors/x", "", "");
        let t = with("font_size 12", &i, F);
        assert_eq!(t, "font_size 12\ninclude ~/.local/state/ostrov/colors/x\n");
        assert_eq!(without(&t, F), "font_size 12\n");
    }

    #[test]
    fn under_a_section() {
        let i = inc("include=~/.local/state/ostrov/colors/x", "[main]", "");
        assert_eq!(with("[main]\nfont=a\n", &i, F), "[main]\ninclude=~/.local/state/ostrov/colors/x\nfont=a\n");
        assert_eq!(with("font=a\n", &i, F), "[main]\ninclude=~/.local/state/ostrov/colors/x\nfont=a\n");
    }

    #[test]
    fn first_in_an_array() {
        let i = inc("", "", "general.import");
        let t = with("[general]\nimport = [\"a.toml\"]\nlive = true\n", &i, F);
        assert!(t.contains(&format!("import = [\"{F}\", \"a.toml\"]")), "{t}");
        assert_eq!(without(&t, F), "[general]\nimport = [\"a.toml\"]\nlive = true\n");
        let alone = with("", &i, F);
        assert!(alone.contains("[general]") && alone.contains(F), "{alone}");
        assert!(!without(&alone, F).contains(F));
    }
}
