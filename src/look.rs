//! The look as config.toml's [appearance] sets it, made into what style.rs draws with: the theme's palette
//! (theme.rs) over style.rs's own, the accent and the surface over the theme's, [colors] over all; the shapes'
//! radii, the density of the control centre's rows (the appearance's, else the theme's suggestion); and what is set
//! outside the CSS: GTK's animations and its dark variant, Hyprland's blur.

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

/// The accent as the appearance says: a colour, or "wallpaper", the wallpaper's own (wallpaper_accent); "" the
/// theme's.
pub fn accent(a: &Appearance) -> String {
    if a.accent == "wallpaper" {
        return wallpaper_accent(crate::theme::light_mode()).unwrap_or_default();
    }
    a.accent.clone()
}

thread_local! {
    /// The wallpaper's accent last worked out: for which picture and side.
    static WALL: std::cell::RefCell<Option<(String, bool, Option<String>)>> = const { std::cell::RefCell::new(None) };
}

/// The wallpaper's accent: of its colours the one there is most of among the vivid ones (its pixels by hue, each
/// as much as it is saturated and bright), made light enough to stand out after dark and dark enough by day.
/// None with no wallpaper, or one with nothing vivid in it.
pub fn wallpaper_accent(light: bool) -> Option<String> {
    let pick = crate::modules::wallpaper::service::pick();
    if !pick.on {
        return None;
    }
    let key = (pick.path.clone(), light);
    if let Some(Some(hit)) = WALL.with(|w| w.borrow().as_ref().filter(|(p, l, _)| (p, *l) == (&key.0, key.1)).map(|w| w.2.clone())) {
        return Some(hit);
    }
    let found = wallpaper_pixels(&pick.path).and_then(|px| accent_of(&px, light));
    WALL.with(|w| *w.borrow_mut() = Some((key.0, key.1, found.clone())));
    found
}

/// A picture's pixels, every one of a grid of about 100x100, through GTK's own loader (gdk-pixbuf's may be
/// sandboxed away).
fn wallpaper_pixels(path: &str) -> Option<Vec<(u8, u8, u8)>> {
    let tex = gtk4::gdk::Texture::from_filename(path).ok()?;
    use gtk4::gdk::prelude::*;
    let (w, h) = (tex.intrinsic_width() as usize, tex.intrinsic_height() as usize);
    let stride = w * 4;
    let mut bytes = vec![0u8; stride * h];
    tex.download(&mut bytes, stride);
    let step = (w.max(h) / 100).max(1);
    let mut pixels = Vec::new();
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            // GDK's download: B, G, R, A in memory on a little-endian machine (cairo's ARGB32)
            let i = y * stride + x * 4;
            pixels.push((bytes[i + 2], bytes[i + 1], bytes[i]));
        }
    }
    Some(pixels)
}

thread_local! {
    /// The wallpaper's source colour last worked out, for which picture.
    static SOURCE: std::cell::RefCell<Option<(String, (u8, u8, u8))>> = const { std::cell::RefCell::new(None) };
}

/// The wallpaper's source colour as Material You picks it (its colours quantized, scored for a theme: the vivid
/// one there is much of); grey with no wallpaper.
fn wallpaper_source() -> (u8, u8, u8) {
    let pick = crate::modules::wallpaper::service::pick();
    let path = if pick.on { pick.path } else { String::new() };
    if let Some(hit) = SOURCE.with(|s| s.borrow().as_ref().filter(|(p, _)| *p == path).map(|s| s.1)) {
        return hit;
    }
    use material_colors::{color::Argb, quantize::{Quantizer, QuantizerCelebi}, score::Score};
    let found = wallpaper_pixels(&path).map(|px| {
        let px: Vec<Argb> = px.into_iter().map(|(r, g, b)| Argb::new(255, r, g, b)).collect();
        let ranked = Score::score(&QuantizerCelebi::quantize(&px, 128).color_to_count, None, Some(Argb::new(255, 128, 128, 128)), None);
        ranked.first().map_or((128, 128, 128), |c| (c.red, c.green, c.blue))
    });
    let found = found.unwrap_or((128, 128, 128));
    SOURCE.with(|s| *s.borrow_mut() = Some((path, found)));
    found
}

/// The wallpaper theme's colours, after dark or by day, as Material You makes them from the wallpaper's source
/// colour: its grounds and text near-neutral with the source's hue in them, at tones that keep text readable
/// whatever the picture; the accent the source's, vivid; the terminal's sixteen One Dark's hues each turned at
/// most 15° toward the source's (harmonized), at a tone set for its ground, so red stays red and every one reads
/// (4.5:1 and over on the terminal's ground).
pub fn wallpaper_theme(light: bool) -> BTreeMap<String, String> {
    theme_of(wallpaper_source(), light)
}

/// The wallpaper theme's colours from a source colour (wallpaper_theme).
fn theme_of((r, g, b): (u8, u8, u8), light: bool) -> BTreeMap<String, String> {
    use material_colors::{blend::harmonize, color::Argb, hct::Hct, palette::TonalPalette};
    let src = Argb::new(255, r, g, b);
    let s: Hct = src.into();
    let (hue, chroma) = (s.get_hue(), s.get_chroma());
    let n = TonalPalette::of(hue, (chroma / 6.0).clamp(4.0, 12.0));
    let nv = TonalPalette::of(hue, (chroma / 4.0).clamp(6.0, 16.0));
    let p = TonalPalette::of(hue, chroma.max(48.0));
    let hex = |c: Argb| format!("#{:02x}{:02x}{:02x}", c.red, c.green, c.blue);
    let rgba = |c: Argb, a: &str| format!("rgba({}, {}, {}, {a})", c.red, c.green, c.blue);
    let t = |pal: &TonalPalette, dark: i32, day: i32| pal.tone(if light { day } else { dark });
    let fg = t(&n, 93, 12);
    let mut c = BTreeMap::new();
    let mut put = |k: &str, v: String| drop(c.insert(k.to_string(), v));
    put("surface", hex(t(&n, 12, 96)));
    put("bar", rgba(t(&n, 6, 92), "ALPHA"));
    put("fg", hex(fg));
    put("dim", hex(t(&nv, 72, 38)));
    put("accent", hex(t(&p, 80, 40)));
    put("ink", hex(t(&p, 20, 100)));
    put("accent-pressed", hex(t(&p, 70, 34)));
    put("accent-rule", hex(t(&p, 60, 28)));
    put("hover", rgba(fg, if light { "0.08" } else { "0.12" }));
    put("raised", rgba(fg, if light { "0.05" } else { "0.07" }));
    put("card", rgba(fg, if light { "0.04" } else { "0.05" }));
    put("well", hex(t(&nv, 24, 88)));
    put("sunk", if light { "#ffffff".into() } else { rgba(t(&n, 4, 4), "0.5") });
    put("rule", hex(t(&nv, 30, 82)));
    put("idle", hex(t(&nv, 45, 62)));
    put("handle", hex(t(&p, 70, 45)));
    let red = TonalPalette::of(25.0, 70.0);
    put("urgent", hex(t(&red, 70, 40)));
    put("recording", hex(t(&red, 65, 45)));
    put("lock", hex(t(&n, 6, 92)));
    put("ground", hex(t(&n, 6, 92)));
    // the terminal's: black, six hues, white; then the same brighter
    const HUES: [(u8, u8, u8); 6] = [(0xe0, 0x6c, 0x75), (0x98, 0xc3, 0x79), (0xe5, 0xc0, 0x7b), (0x61, 0xaf, 0xef), (0xc6, 0x78, 0xdd), (0x56, 0xb6, 0xc2)];
    let hued = |(r, g, b): (u8, u8, u8), tone: f64| -> Argb {
        let h: Hct = harmonize(Argb::new(255, r, g, b), src).into();
        Hct::from(h.get_hue(), h.get_chroma().max(40.0), tone).into()
    };
    let mut ansi = vec![t(&n, 20, 20)];
    ansi.extend(HUES.map(|h| hued(h, if light { 42.0 } else { 72.0 })));
    ansi.push(t(&n, 80, 88));
    ansi.push(t(&n, 50, 50));
    ansi.extend(HUES.map(|h| hued(h, if light { 34.0 } else { 82.0 })));
    ansi.push(t(&n, 98, 98));
    for (i, a) in ansi.into_iter().enumerate() {
        put(&format!("ansi{i}"), hex(a));
    }
    c
}

/// The accent of a picture's pixels (see wallpaper_accent).
fn accent_of(pixels: &[(u8, u8, u8)], light: bool) -> Option<String> {
    // 36 hues of 10°, each with its weight and its colours summed
    let mut bins = [(0.0f64, 0.0f64, 0.0f64, 0.0f64); 36];
    for &(r, g, b) in pixels {
        let (h, s, v) = hsv(r, g, b);
        let w = s * s * v;
        if s < 0.2 || v < 0.15 {
            continue;
        }
        let bin = &mut bins[(h / 10.0) as usize % 36];
        bin.0 += w;
        bin.1 += h * w;
        bin.2 += s * w;
        bin.3 += v * w;
    }
    // the heaviest hue with its neighbours
    let total = |i: usize| bins[(i + 35) % 36].0 + bins[i].0 + bins[(i + 1) % 36].0;
    let best = (0..36).max_by(|&a, &b| total(a).total_cmp(&total(b)))?;
    // that hue and its neighbours together, the hue an angle near best's (across 0° kept whole)
    let (mut w, mut hw, mut sw) = (0.0, 0.0, 0.0);
    for i in [(best + 35) % 36, best, (best + 1) % 36] {
        let (bw, bh, bs, _) = bins[i];
        let mean = if bw > 0.0 { bh / bw } else { 0.0 };
        let near = mean + 360.0 * ((best as f64 * 10.0 - mean) / 360.0).round();
        w += bw;
        hw += near * bw;
        sw += bs;
    }
    if w <= 0.0 {
        return None;
    }
    let (h, s) = ((hw / w).rem_euclid(360.0), (sw / w).clamp(0.45, 0.85));
    // after dark light, by day deep: readable on either ground
    let v = if light { 0.62 } else { 0.92 };
    let (r, g, b) = from_hsv(h, s, v);
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

fn hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

fn from_hsv(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |u: f64| ((u + m) * 255.0).round() as u8;
    (to(r), to(g), to(b))
}

/// The palette's colours over style.rs's, later over earlier: the theme's, its surface's colour (or the
/// appearance's) under the appearance's opacity, the bar's colour if the appearance has one (its opacity, ALPHA,
/// the wallpaper's: bar_alpha), the appearance's accent, then the config's [colors].
pub fn palette(a: &Appearance, t: &Theme, colors: &BTreeMap<String, String>) -> String {
    let def = |k: &str, v: &str| format!("@define-color {k} {v};\n");
    // the terminal's sixteen (the wallpaper theme's) are the apps', not the CSS's
    let mut out: String =
        t.colors.iter().filter(|(k, _)| *k != "surface" && !k.starts_with("ansi")).map(|(k, v)| def(k, v)).collect();
    let theirs = t.colors.get("surface").and_then(|s| rgb(s));
    let (r, g, b) = rgb(&a.surface).or(theirs).unwrap_or((0, 0, 0));
    out += &def("surface", &format!("rgba({r}, {g}, {b}, {:.2})", a.opacity.clamp(0.0, 1.0)));
    // the same, solid: under what is drawn over a surface (a hover)
    out += &def("surface-solid", &format!("rgb({r}, {g}, {b})"));
    if let Some((r, g, b)) = rgb(&a.bar_color) {
        out += &def("bar", &format!("rgba({r}, {g}, {b}, ALPHA)"));
    }
    // with no wallpaper the desktop the bar's colour, solid: one colour from the bar down
    let bar = rgb(&a.bar_color).or_else(|| t.colors.get("bar").and_then(|b| rgb(&b.replace("ALPHA", "1"))));
    if let Some((r, g, b)) = bar {
        out += &def("ground", &format!("rgb({r}, {g}, {b})"));
    }
    if let Some((r, g, b)) = rgb(&accent(a)) {
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
    match a.density.clone().or_else(theirs).as_deref().unwrap_or("compact") {
        "normal" => (48, 8),
        "comfortable" => (56, 10),
        _ => (40, 6),
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

/// What the appearance sets outside the CSS: GTK's animations and dark variant; Hyprland's blur and the layers'
/// rules, in the file hyprland.conf sources (modules/hyprland.rs), written again only when they change.
pub fn apply(a: &Appearance, t: &Theme) {
    if let Some(s) = gtk4::Settings::default() {
        s.set_gtk_enable_animations(a.animations);
        s.set_gtk_application_prefer_dark_theme(t.dark);
    }
    if crate::wm::wm() == crate::wm::Wm::Hyprland {
        crate::modules::hyprland::put_rules();
    }
}

/// Hyprland's blur options the appearance (or its theme) sets, as decoration:blur keywords and their values;
/// what neither sets left to Hyprland's own config.
pub fn blur(a: &Appearance, t: &Theme) -> Vec<(&'static str, String)> {
    let flag = |b: Option<bool>| b.map(|b| (b as u8).to_string());
    let num = |n: Option<f64>| n.map(|n| format!("{n:.4}"));
    [
        ("enabled", flag(a.blur.or(t.blur))),
        ("size", a.blur_size.map(|n| n.to_string())),
        ("passes", a.blur_passes.map(|n| n.to_string())),
        ("vibrancy", num(a.blur_vibrancy)),
        ("contrast", num(a.blur_contrast)),
        ("brightness", num(a.blur_brightness)),
        ("noise", num(a.blur_noise)),
    ]
    .into_iter()
    .filter_map(|(k, v)| Some((k, v?)))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallpaper_theme_reads() {
        use material_colors::contrast::ratio_of_tones;
        let tone = |c: &str| {
            let v = u32::from_str_radix(&c[1..7], 16).unwrap();
            material_colors::color::Argb::from_u32(0xff00_0000 | v).as_lstar()
        };
        let ground = |c: &str| c.trim_start_matches("rgba(").split(", ").take(3).map(|n| format!("{:02x}", n.parse::<u8>().unwrap())).collect::<String>();
        // a warm sunset, a pale beige, a deep blue, a green, grey: on either side the text and the terminal's colours
        // (its black, white and greys aside) read on their grounds
        for src in [(0xb0, 0x55, 0x28), (0xf2, 0xdb, 0xab), (0x20, 0x30, 0xa0), (0x30, 0xa0, 0x40), (128, 128, 128)] {
            for light in [false, true] {
                let t = theme_of(src, light);
                let bar = tone(&format!("#{}", ground(&t["bar"])));
                assert!(ratio_of_tones(tone(&t["fg"]), tone(&t["surface"])) >= 7.0, "{src:?} {light}: text");
                for i in (1..7).chain(9..15) {
                    let r = ratio_of_tones(tone(&t[&format!("ansi{i}")]), bar);
                    assert!(r >= 4.5, "{src:?} {light}: ansi{i} {r:.1}:1");
                }
            }
        }
    }

    #[test]
    fn a_pictures_accent() {
        // a picture mostly a deep blue, a little red: the blue, made light after dark
        let mut px = vec![(20, 40, 160); 900];
        px.extend(vec![(200, 30, 30); 100]);
        px.extend(vec![(128, 128, 128); 500]);
        let a = accent_of(&px, false).unwrap();
        let (r, g, b) = rgb(&a).unwrap();
        assert!(b > r && b > g && b > 200, "{a}");
        // greys alone: none
        assert_eq!(accent_of(&[(100, 100, 100); 50], false), None);
    }

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
