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
        include_str!("../i18n/ru/welcome.toml"),
    ],
)];

static WORDS: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
static LANG: OnceLock<String> = OnceLock::new();

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

/// GTK's own words (a calendar's months and weekdays) in the language too: LC_TIME a locale of it the system
/// has, when the language is not the locale's. Before GTK starts, while ostrov is one thread.
pub fn follow_locale() {
    let lang = lang();
    let now = std::env::var("LC_TIME").or_else(|_| std::env::var("LANG")).unwrap_or_default();
    if lang.is_empty() || now.starts_with(lang) {
        return;
    }
    let Ok(out) = std::process::Command::new("locale").arg("-a").output() else { return };
    let all = String::from_utf8_lossy(&out.stdout);
    let mine = all.lines().filter(|l| l.starts_with(&format!("{lang}_")) && l.to_lowercase().contains("utf"));
    let region = format!("{lang}_{}", lang.to_uppercase());
    let pick = mine.clone().find(|l| l.starts_with(&region)).or_else(|| mine.clone().next());
    if let Some(l) = pick {
        // SAFETY: called first thing in main, no other thread reads the environment yet
        unsafe { std::env::set_var("LC_TIME", l) };
    }
}

/// The text in the user's language; a text not 'static (a schema's, sent as JSON) comes back as long as it lives.
pub fn t<'a>(en: &'a str) -> &'a str {
    WORDS.get_or_init(|| catalogue(lang())).get(en).copied().unwrap_or(en)
}

fn lang() -> &'static str {
    LANG.get_or_init(language)
}

/// Of a count's three forms (English texts, each looked up as t() does), the one its number takes: Russian's one
/// (1, 21), few (2-4, 22) or many (5-20, 11); English's forms[0] for 1, forms[2] else. forms[1], never shown in
/// English, is the plural with "|few" after it, a key of its own: plural(n, ["{} event", "{} events|few",
/// "{} events"]), its {} filled with fill() as any.
pub fn plural(n: i64, forms: [&'static str; 3]) -> &'static str {
    t(forms[form(lang(), n)])
}

fn form(lang: &str, n: i64) -> usize {
    let n = n.unsigned_abs();
    match lang {
        "ru" if n % 10 == 1 && n % 100 != 11 => 0,
        "ru" if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) => 1,
        "ru" => 2,
        _ if n == 1 => 0,
        _ => 2,
    }
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

    #[test]
    fn plural_forms_by_language() {
        let ru: Vec<usize> = [0, 1, 2, 4, 5, 11, 12, 14, 21, 22, 25, 101, 111, -3].map(|n| form("ru", n)).into();
        assert_eq!(ru, [2, 0, 1, 1, 2, 2, 2, 2, 0, 1, 2, 0, 2, 1]);
        assert_eq!([0, 1, 2, 21].map(|n| form("en", n)), [2, 0, 2, 2]);
    }
}
