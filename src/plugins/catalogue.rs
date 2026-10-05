//! The catalogue: plugins known by name, installable without their URL (`ostrov plugin install NAME`), listed by
//! `ostrov plugin catalogue`; and the Settings page "Plugins": every plugin found with its switch (an official one
//! off until turned on), the catalogue's not installed yet with Install, and any plugin by its URL or path.

use std::rc::Rc;

use gtk4::glib;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{found, install, word};
use crate::i18n::{fill, t};
use crate::settings::{Action, Field, Kind, Schema, Section};

/// A plugin the catalogue knows: its source a git URL, `#path` a directory in it.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub source: String,
}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    plugin: Vec<Entry>,
}

/// The catalogue ostrov ships with.
// ponytail: built in, new entries come with a release; a remote index fetched from the repository once it is
// published, merged over this one by id
pub fn entries() -> Vec<Entry> {
    toml::from_str::<File>(include_str!("../../catalogue.toml")).map(|f| f.plugin).unwrap_or_default()
}

/// `ostrov plugin catalogue`: every plugin found and whether it runs, then the catalogue's not installed.
pub fn list() -> String {
    let cfg = crate::config::load();
    let have = found();
    let mut lines: Vec<String> = have
        .iter()
        .map(|(_, m, official)| {
            let on = if super::enabled(&cfg, &m.id, *official) { "on" } else { "off" };
            let whose = if *official { "official" } else { "installed" };
            format!("{:<16} {:<10} {:<4} {}", m.id, whose, on, m.description)
        })
        .collect();
    for e in entries().iter().filter(|e| !have.iter().any(|(_, m, _)| m.id == e.id)) {
        lines.push(format!("{:<16} {:<10} {:<4} {} ({})", e.id, "available", "", e.description, e.source));
    }
    lines.join("\n")
}

/// What `ostrov plugin install SOURCE` installs: an official plugin's id turns it on; a catalogue's id its source;
/// anything else (a path, a URL) itself.
pub enum Target {
    Official(String),
    Source(String),
}

pub fn resolve(src: &str) -> Result<Target, String> {
    if !word(src) || std::path::Path::new(src).exists() {
        return Ok(Target::Source(src.into()));
    }
    if found().iter().any(|(_, m, official)| *official && m.id == src) {
        return Ok(Target::Official(src.into()));
    }
    let e = entries().into_iter().find(|e| e.id == src);
    e.map(|e| Target::Source(e.source))
        .ok_or_else(|| fill(t("no plugin {} in the catalogue (ostrov plugin catalogue lists it)"), &[&src]))
}

/// An official plugin turned on in the config, which starts it.
pub fn turn_on(id: &str) -> Result<String, String> {
    crate::settings::write(&format!("plugin.{id}"), "enabled", Some(&json!(true)), false)?;
    Ok(fill(t("plugin {} turned on; its widgets join the gallery after `ostrov restart`"), &[&id]))
}

/// An Install button: the field's value (a source) installed, what came of it said beside the button.
fn install_action() -> Action {
    let run: crate::settings::Run = Rc::new(|v: &Value, done: Rc<dyn Fn(Result<String, String>)>| {
        let src = v.as_str().unwrap_or_default().trim().to_string();
        if src.is_empty() {
            return done(Err(t("a git URL or a path first").into()));
        }
        glib::spawn_future_local(async move { done(install::install(src).await) });
    });
    Action { label: "Install".into(), id: "install".into(), run: Some(run) }
}

/// The Settings page "Plugins", registered as ostrov starts.
pub fn page() {
    let have = found();
    let mut sections: Vec<Section> = have
        .iter()
        .map(|(dir, m, official)| {
            let whose = if *official { t("one of ostrov's own").to_string() } else { dir.display().to_string() };
            let on = Field::new("enabled", "On", Kind::Bool).default(!official);
            let help = if m.description.is_empty() { whose } else { format!("{} — {whose}", m.description) };
            Section::new(&format!("plugin.{}", m.id), &m.name, vec![on]).help(&help)
        })
        .collect();
    for e in entries().iter().filter(|e| !have.iter().any(|(_, m, _)| m.id == e.id)) {
        let mut from = Field::new("source", "Source", Kind::Url).default(e.source.clone());
        from.actions.push(install_action());
        sections.push(Section::new(&format!("plugin.{}", e.id), &e.name, vec![from]).help(&e.description));
    }
    let mut any = Field::new("install", "A git URL or a path", Kind::String)
        .help("a directory with a manifest.toml; a URL's #path a directory in it");
    any.actions.push(install_action());
    sections.push(Section::new("plugins", "Install a plugin", vec![any]));
    crate::settings::register("plugins", t("Plugins"), "application-x-addon-symbolic", Schema { sections });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_reads() {
        let all = entries();
        assert!(!all.is_empty(), "catalogue.toml reads");
        for e in &all {
            assert!(word(&e.id), "{} is a plugin's id", e.id);
            assert!(install::is_url(&e.source), "{}'s source is a URL", e.id);
        }
    }
}
