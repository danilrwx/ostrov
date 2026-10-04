//! ostrov's words in the user's language: every text it shows written in English and looked up as it is shown,
//! t("Wi-Fi"), a catalogue of a language's texts by their English ones (i18n/LANG/*.toml, "English" = "theirs"),
//! the English itself where it has none. A text with values in it says {} for each, fill() putting them in, so
//! a language may move them: t("{} min left"). The language is [appearance] language, else the locale's
//! (LC_ALL, LC_MESSAGES, LANG), else English; taken at the start.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Each language's catalogue, its parts by what of ostrov they hold.
const CATALOGUES: &[(&str, &[&str])] = &[(
    "ru",
    &[
        include_str!("../i18n/ru/core.toml"),
        include_str!("../i18n/ru/cc.toml"),
        include_str!("../i18n/ru/settings.toml"),
        include_str!("../i18n/ru/modules.toml"),
        include_str!("../i18n/ru/shell.toml"),
    ],
)];

static WORDS: OnceLock<HashMap<String, &'static str>> = OnceLock::new();

/// The language: the config's, the locale's, its two letters ("ru" of ru_RU.UTF-8).
pub fn language() -> String {
    let set = crate::config::load().appearance.language;
    let from = if set.is_empty() {
        ["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|k| std::env::var(k).ok()).find(|v| !v.is_empty())
    } else {
        Some(set)
    };
    from.map(|l| l.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_lowercase()).unwrap_or_default()
}

fn catalogue(lang: &str) -> HashMap<String, &'static str> {
    let parts = CATALOGUES.iter().find(|(l, _)| *l == lang).map_or(&[][..], |(_, p)| *p);
    let mut out = HashMap::new();
    for part in parts {
        match toml::from_str::<HashMap<String, String>>(part) {
            Ok(words) => out.extend(words.into_iter().map(|(k, v)| (k, &*Box::leak(v.into_boxed_str())))),
            Err(e) => eprintln!("ostrov: i18n/{lang}: {e}"),
        }
    }
    out
}

/// The text in the user's language.
pub fn t(en: &'static str) -> &'static str {
    WORDS.get_or_init(|| catalogue(&language())).get(en).copied().unwrap_or(en)
}

/// The text's {} filled with the values, in order.
pub fn fill(text: &str, values: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut values = values.iter();
    let mut rest = text;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(v) = values.next() {
            out.push_str(&v.to_string());
        }
        rest = &rest[i + 2..];
    }
    out + rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_reads_and_keeps_its_values() {
        for (lang, _) in CATALOGUES {
            for (en, theirs) in catalogue(lang) {
                assert_eq!(en.matches("{}").count(), theirs.matches("{}").count(), "{lang}: {en}");
            }
        }
    }

    #[test]
    fn values_filled_in_order() {
        assert_eq!(fill("{} h {} min left", &[&2, &"5"]), "2 h 5 min left");
        assert_eq!(fill("none", &[]), "none");
    }
}
