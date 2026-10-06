//! The look as config.toml's [appearance] sets it, made into what style.rs draws with: the theme's palette
//! (theme.rs) over style.rs's own, the accent and the surface over the theme's, [colors] over all; the shapes'
//! radii, the density of the control centre's rows (the appearance's, else the theme's suggestion); and what is set
//! outside the CSS: GTK's animations and its dark variant, Hyprland's blur.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::config::Appearance;
use crate::theme::Theme;

/// The accents the Appearance page offers as swatches.
pub const ACCENTS: &[&str] =
    &["#ffffff", "#0a84ff", "#bf5af2", "#ff375f", "#ff453a", "#ff9f0a", "#ffd60a", "#30d158", "#8e8e93"];

fn rgb(c: &str) -> Option<(u8, u8, u8)> {
    let c = gtk4::gdk::RGBA::parse(c).ok()?;
    let b = |x: f32| (x * 255.0).round() as u8;
    Some((b(c.red()), b(c.green()), b(c.blue())))
}

/// The palette's colours over style.rs's, later over earlier: the theme's, its surface's colour (or the
/// appearance's) under the appearance's opacity, the bar's colour if the appearance has one (its opacity, ALPHA,
/// the wallpaper's: bar_alpha), the appearance's accent, then the config's [colors].
pub fn palette(a: &Appearance, t: &Theme, colors: &BTreeMap<String, String>) -> String {
    let def = |k: &str, v: &str| format!("@define-color {k} {v};\n");
    let mut out: String = t.colors.iter().filter(|(k, _)| *k != "surface").map(|(k, v)| def(k, v)).collect();
    let theirs = t.colors.get("surface").and_then(|s| rgb(s));
    let (r, g, b) = rgb(&a.surface).or(theirs).unwrap_or((0, 0, 0));
    out += &def("surface", &format!("rgba({r}, {g}, {b}, {:.2})", a.opacity.clamp(0.0, 1.0)));
    // the same, solid: under what is drawn over a surface (a hover)
    out += &def("surface-solid", &format!("rgb({r}, {g}, {b})"));
    if let Some((r, g, b)) = rgb(&a.bar_color) {
        out += &def("bar", &format!("rgba({r}, {g}, {b}, ALPHA)"));
    }
    if let Some((r, g, b)) = rgb(&a.accent) {
        // what goes on the accent black or white, as the accent is light or dark
        let light = 0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64 > 150.0;
        out += &format!(
            "@define-color accent rgb({r}, {g}, {b});\n@define-color ink {};\n\
             @define-color accent-pressed shade(@accent, 0.85);\n@define-color accent-rule shade(@accent, 0.7);\n",
            if light { "#000000" } else { "#ffffff" }
        );
    }
    out + &colors.iter().map(|(k, v)| def(k, v)).collect::<String>()
}

/// A surface's corner radius: the appearance's, else the theme's, else 10.
pub fn radius(a: &Appearance, t: &Theme) -> u32 {
    a.radius.or(t.radius).unwrap_or(10)
}

/// The CSS's radii made the appearance's: a surface's 10 px r, what is on it 6 px r - 4. Only the radii
/// change; any other 10px or 6px stays.
/// The rules sized as [appearance] says: every font size scaled by font_size over the 11pt they are written for
/// (the lock's clock and all), the family set if it says one; icons, the bar's icons, its blocks' padding.
pub fn sizes(css: &str, a: &Appearance) -> String {
    let k = a.font_size.clamp(6.0, 32.0) / 11.0;
    let mut out = String::with_capacity(css.len() + 256);
    let mut rest = css;
    while let Some(i) = rest.find("font-size:") {
        let (head, tail) = rest.split_at(i + "font-size:".len());
        out.push_str(head);
        let end = tail.find([';', '}']).unwrap_or(tail.len());
        let v = tail[..end].trim();
        match v.strip_suffix("pt").and_then(|n| n.parse::<f64>().ok()) {
            Some(n) => out.push_str(&format!(" {:.1}pt", n * k)),
            None => out.push_str(&tail[..end]),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    if !a.font.trim().is_empty() {
        out.push_str(&format!("* {{ font-family: \"{}\"; }}\n", a.font.trim().replace(['"', '\\', ';', '{', '}'], "")));
    }
    out.push_str(&format!(
        "image {{ -gtk-icon-size: {}px; }}\n.pill image {{ -gtk-icon-size: {}px; }}\n\
         .pill {{ padding-left: {p}px; padding-right: {p}px; }}\n",
        a.icon_size.clamp(8, 64),
        a.bar_icon_size.clamp(8, 64),
        p = a.bar_padding.min(48),
    ));
    out
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn fonts_scale_and_icons_follow() {
        let a = Appearance { font_size: 13.2, font: "Inter".into(), bar_icon_size: 14, ..Appearance::default() };
        let css = sizes(".a { font-size: 11pt; } .b { font-size: 8.5pt; color: x; } .c { font-size: 1em; }", &a);
        assert!(css.contains(".a { font-size: 13.2pt; }"), "{css}");
        assert!(css.contains("font-size: 10.2pt;"), "{css}");
        assert!(css.contains("font-size: 1em;"), "{css}");
        assert!(css.contains("font-family: \"Inter\""));
        assert!(css.contains(".pill image { -gtk-icon-size: 14px; }"));
    }
}

pub fn radii(css: &str, r: u32) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(i) = rest.find("radius:") {
        let (head, tail) = rest.split_at(i + "radius:".len());
        out.push_str(head);
        let end = tail.find([';', '}']).unwrap_or(tail.len());
        let v: Vec<String> = tail[..end]
            .split(' ')
            .map(|t| match t {
                "10px" => format!("{r}px"),
                "6px" => format!("{}px", r.saturating_sub(4)),
                t => t.to_string(),
            })
            .collect();
        out.push_str(&v.join(" "));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// The control centre's row height and gap: the appearance's density, else its theme's.
pub fn density(a: &Appearance) -> (i32, i32) {
    let theirs = || crate::theme::get(&a.theme).density;
    match a.density.clone().or_else(theirs).as_deref().unwrap_or("normal") {
        "compact" => (40, 6),
        "comfortable" => (56, 10),
        _ => (48, 8),
    }
}

/// The Appearance page's previews: each theme's card on its surface in its text's colour with its accent's dot,
/// each swatch its colour.
pub fn previews(themes: &[Theme]) -> String {
    let mut out = String::new();
    for t in themes {
        let get = |k: &str| t.colors.get(k).map_or("#ffffff", String::as_str);
        let (r, g, b) = t.colors.get("surface").and_then(|s| rgb(s)).unwrap_or((0, 0, 0));
        out += &format!(
            "button.theme-{id} {{ background: rgb({r}, {g}, {b}); }}\nbutton.theme-{id} label {{ color: {}; }}\n\
             button.theme-{id} .theme-dot {{ background: {}; }}\n",
            get("fg"),
            get("accent"),
            id = t.id,
        );
    }
    for (i, c) in ACCENTS.iter().enumerate() {
        out += &format!("button.swatch-{i} {{ background: {c}; }}\n");
    }
    out
}

thread_local! {
    /// Hyprland's blur as last set from here: its keywords' values.
    static BLUR: RefCell<Vec<(&'static str, Option<String>)>> = const { RefCell::new(Vec::new()) };
    static RULE: RefCell<String> = const { RefCell::new(String::new()) };
}

/// What the appearance sets outside the CSS: GTK's animations and dark variant, and Hyprland's blur where the file
/// or the theme gives it and it changed (so Hyprland's own config rules while neither says anything).
pub fn apply(a: &Appearance, t: &Theme) {
    if let Some(s) = gtk4::Settings::default() {
        s.set_gtk_enable_animations(a.animations);
        s.set_gtk_application_prefer_dark_theme(t.dark);
    }
    if crate::wm::wm() != crate::wm::Wm::Hyprland {
        return;
    }
    // the layer's blur threshold under the opacity as it changes: a later rule over the one before
    let rule = crate::modules::hyprland::rule();
    if RULE.with(|r| r.replace(rule.clone())) != rule && crate::config::load().hyprland.rules {
        drop(crate::wm::hyprctl(&format!("keyword layerrule {rule}")));
    }
    let flag = |b: Option<bool>| b.map(|b| (b as u8).to_string());
    let num = |n: Option<f64>| n.map(|n| format!("{n:.4}"));
    let now: Vec<(&'static str, Option<String>)> = vec![
        ("enabled", flag(a.blur.or(t.blur))),
        ("size", a.blur_size.map(|n| n.to_string())),
        ("passes", a.blur_passes.map(|n| n.to_string())),
        ("vibrancy", num(a.blur_vibrancy)),
        ("contrast", num(a.blur_contrast)),
        ("brightness", num(a.blur_brightness)),
        ("noise", num(a.blur_noise)),
    ];
    let last = BLUR.with(|b| b.replace(now.clone()));
    for (k, v) in &now {
        let was = last.iter().find(|(l, _)| l == k).and_then(|(_, v)| v.as_ref());
        if let Some(v) = v.as_ref().filter(|v| Some(*v) != was) {
            drop(crate::wm::hyprctl(&format!("keyword decoration:blur:{k} {v}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radii_only() {
        let css = ".a { border-radius: 10px; padding: 10px; } .b { border-radius: 0 6px 6px 0; min-width: 16px; }\n\
                   .c { border-top-left-radius: 10px; border-radius: 9px; }";
        assert_eq!(
            radii(css, 14),
            ".a { border-radius: 14px; padding: 10px; } .b { border-radius: 0 10px 10px 0; min-width: 16px; }\n\
             .c { border-top-left-radius: 14px; border-radius: 9px; }"
        );
        assert_eq!(radii(css, 10), css);
    }

    fn theme(text: &str) -> Theme {
        crate::theme::parse(text, "t", String::new()).expect("theme.toml")
    }

    #[test]
    fn accents_inked() {
        let (dark, none) = (theme("name = \"D\"\ndark = true\n"), BTreeMap::new());
        let a = |accent: &str| Appearance { accent: accent.into(), ..Appearance::default() };
        assert!(palette(&a("#ffd60a"), &dark, &none).contains("ink #000000"));
        assert!(palette(&a("#0a3d91"), &dark, &none).contains("ink #ffffff"));
        assert_eq!(palette(&a(""), &dark, &none), "@define-color surface rgba(0, 0, 0, 0.75);\n@define-color surface-solid rgb(0, 0, 0);\n");
    }

    /// The value a token ends up with: its last definition, as GTK takes it.
    fn last<'a>(css: &'a str, token: &str) -> &'a str {
        let def = format!("@define-color {token} ");
        css.lines().rev().find_map(|l| l.strip_prefix(&def)?.strip_suffix(';')).unwrap_or("")
    }

    /// style.rs's palette, under the theme's, under [appearance]'s accent and surface, under [colors].
    #[test]
    fn merge_order() {
        let t = theme(
            "name = \"T\"\ndark = true\nradius = 4\n[colors]\nfg = \"#010101\"\naccent = \"#020202\"\n\
             dim = \"#030303\"\nsurface = \"#040404\"\n",
        );
        let colors = BTreeMap::from([("dim".to_string(), "#0a0a0a".to_string())]);
        let mut a = Appearance { theme: "t".into(), opacity: 0.9, ..Appearance::default() };
        let css = palette(&a, &t, &colors);
        assert_eq!(last(&css, "fg"), "#010101");
        assert_eq!(last(&css, "accent"), "#020202");
        assert_eq!(last(&css, "dim"), "#0a0a0a");
        assert_eq!(last(&css, "surface"), "rgba(4, 4, 4, 0.90)");
        assert_eq!(last(&css, "rule"), "", "left to style.rs's");
        a.accent = "#ff0000".into();
        a.surface = "#102030".into();
        let css = palette(&a, &t, &colors);
        assert_eq!((last(&css, "accent"), last(&css, "ink")), ("rgb(255, 0, 0)", "#ffffff"));
        assert_eq!(last(&css, "surface"), "rgba(16, 32, 48, 0.90)");
        let colors = BTreeMap::from([("accent".to_string(), "#00ff00".to_string())]);
        assert_eq!(last(&palette(&a, &t, &colors), "accent"), "#00ff00");
        assert_eq!(radius(&a, &t), 4);
        a.radius = Some(14);
        assert_eq!(radius(&a, &t), 14);
        assert_eq!(radius(&Appearance::default(), &Theme::default()), 10);
    }

    #[test]
    fn previews_per_theme() {
        let nord = crate::theme::parse(include_str!("../themes/nord/theme.toml"), "nord", String::new());
        let css = previews(&[nord.expect("nord")]);
        assert!(css.contains("button.theme-nord { background: rgb(53, 60, 74); }"), "{css}");
        assert!(css.contains("button.theme-nord .theme-dot { background: #88c0d0; }"));
    }
}
