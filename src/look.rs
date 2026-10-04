//! The look as config.toml's [appearance] sets it, made into what style.rs draws with: a theme's palette over
//! style.rs's own (dark), the accent and the surface over the theme's, the shapes' radii, the density of the
//! control centre's rows; and what is set outside the CSS: GTK's animations, Hyprland's blur.

use std::cell::RefCell;

use crate::config::Appearance;

/// The themes, in the order the Appearance page shows them.
pub const THEMES: &[&str] = &["dark", "light", "graphite", "nord", "solarized"];

/// A theme: its surface's colour (the opacity is the user's), the palette's colours over style.rs's.
fn theme(name: &str) -> ((u8, u8, u8), &'static [(&'static str, &'static str)]) {
    match name {
        "light" => ((246, 246, 246), &[
            ("fg", "#1d1d1f"), ("dim", "#6e6e73"), ("ink", "#ffffff"), ("accent", "#1d1d1f"),
            ("accent-rule", "#555555"), ("accent-pressed", "#3a3a3c"), ("bar", "rgba(242, 242, 242, ALPHA)"),
            ("hover", "rgba(0, 0, 0, 0.08)"), ("raised", "rgba(0, 0, 0, 0.05)"), ("card", "rgba(0, 0, 0, 0.04)"),
            ("well", "rgba(0, 0, 0, 0.1)"), ("sunk", "rgba(255, 255, 255, 0.7)"), ("rule", "#d2d2d7"),
            ("idle", "#aaaaaa"),
        ]),
        "graphite" => ((36, 36, 40), &[
            ("fg", "#e8e8ea"), ("dim", "#8e8e93"), ("ink", "#1c1c1e"), ("accent", "#c8c8cc"),
            ("accent-rule", "#8e8e93"), ("accent-pressed", "#a8a8ac"), ("bar", "rgba(28, 28, 30, ALPHA)"),
            ("hover", "rgba(255, 255, 255, 0.12)"), ("rule", "#48484a"), ("idle", "#636366"),
        ]),
        "nord" => ((46, 52, 64), &[
            ("fg", "#eceff4"), ("dim", "#9aa5b8"), ("ink", "#2e3440"), ("accent", "#88c0d0"),
            ("accent-rule", "#5e81ac"), ("accent-pressed", "#81a1c1"), ("bar", "rgba(46, 52, 64, ALPHA)"),
            ("rule", "#4c566a"), ("idle", "#4c566a"), ("urgent", "#bf616a"),
        ]),
        "solarized" => ((0, 43, 54), &[
            ("fg", "#eee8d5"), ("dim", "#839496"), ("ink", "#002b36"), ("accent", "#b58900"),
            ("accent-rule", "#8a6a00"), ("accent-pressed", "#9c7600"), ("bar", "rgba(0, 43, 54, ALPHA)"),
            ("rule", "#2a4f5a"), ("idle", "#586e75"), ("urgent", "#dc322f"),
        ]),
        _ => ((0, 0, 0), &[]),
    }
}

/// The accents the Appearance page offers as swatches.
pub const ACCENTS: &[&str] =
    &["#ffffff", "#0a84ff", "#bf5af2", "#ff375f", "#ff453a", "#ff9f0a", "#ffd60a", "#30d158", "#8e8e93"];

fn rgb(c: &str) -> Option<(u8, u8, u8)> {
    let c = gtk4::gdk::RGBA::parse(c).ok()?;
    let b = |x: f32| (x * 255.0).round() as u8;
    Some((b(c.red()), b(c.green()), b(c.blue())))
}

/// The palette's colours as the appearance sets them, to go between style.rs's palette and [colors].
pub fn palette(a: &Appearance) -> String {
    let (surface, colors) = theme(&a.theme);
    let mut out: String = colors.iter().map(|(k, v)| format!("@define-color {k} {v};\n")).collect();
    let (r, g, b) = rgb(&a.surface).unwrap_or(surface);
    out += &format!("@define-color surface rgba({r}, {g}, {b}, {:.2});\n", a.opacity.clamp(0.0, 1.0));
    if let Some((r, g, b)) = rgb(&a.accent) {
        // what goes on the accent black or white, as the accent is light or dark
        let light = 0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64 > 150.0;
        out += &format!(
            "@define-color accent rgb({r}, {g}, {b});\n@define-color ink {};\n\
             @define-color accent-pressed shade(@accent, 0.85);\n@define-color accent-rule shade(@accent, 0.7);\n",
            if light { "#000000" } else { "#ffffff" }
        );
    }
    out
}

/// The CSS's radii made the appearance's: a surface's 10 px r, what is on it 6 px r - 4. Only the radii
/// change; any other 10px or 6px stays.
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

/// The control centre's row height and gap for a density.
pub fn density(name: &str) -> (i32, i32) {
    match name {
        "compact" => (40, 6),
        "comfortable" => (56, 10),
        _ => (48, 8),
    }
}

/// The Appearance page's previews: each theme's card on its surface with its accent, each swatch its colour.
pub fn previews() -> String {
    let mut out = String::new();
    for t in THEMES {
        let ((r, g, b), colors) = theme(t);
        let get = |k: &str, or: &'static str| colors.iter().find(|(n, _)| *n == k).map_or(or, |(_, v)| *v);
        out += &format!(
            ".theme-{t} {{ background: rgb({r}, {g}, {b}); }}\n.theme-{t} label {{ color: {}; }}\n\
             .theme-{t} .theme-dot {{ background: {}; }}\n",
            get("fg", "#ffffff"),
            get("accent", "#ffffff")
        );
    }
    for (i, c) in ACCENTS.iter().enumerate() {
        out += &format!("button.swatch-{i} {{ background: {c}; }}\n");
    }
    out
}

thread_local! {
    /// Hyprland's blur as last set from here: enabled, size, passes.
    static BLUR: RefCell<(Option<bool>, Option<u32>, Option<u32>)> = const { RefCell::new((None, None, None)) };
}

/// What the appearance sets outside the CSS: GTK's animations, and Hyprland's blur where the file gives it and
/// it changed (so Hyprland's own config rules while the file says nothing).
pub fn apply(a: &Appearance) {
    if let Some(s) = gtk4::Settings::default() {
        s.set_gtk_enable_animations(a.animations);
    }
    if crate::wm::wm() != crate::wm::Wm::Hyprland {
        return;
    }
    let now = (a.blur, a.blur_size, a.blur_passes);
    let last = BLUR.with(|b| b.replace(now));
    let set = |k: &str, v: String| drop(crate::wm::hyprctl(&format!("keyword decoration:blur:{k} {v}")));
    if now.0 != last.0 {
        if let Some(on) = now.0 {
            set("enabled", (on as u8).to_string());
        }
    }
    if now.1 != last.1 {
        if let Some(n) = now.1 {
            set("size", n.to_string());
        }
    }
    if now.2 != last.2 {
        if let Some(n) = now.2 {
            set("passes", n.to_string());
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

    #[test]
    fn accents_inked() {
        let a = |accent: &str| Appearance { accent: accent.into(), ..Appearance::default() };
        assert!(palette(&a("#ffd60a")).contains("ink #000000"));
        assert!(palette(&a("#0a3d91")).contains("ink #ffffff"));
        assert!(palette(&a("")).ends_with("@define-color surface rgba(0, 0, 0, 0.75);\n"));
        let nord = Appearance { theme: "nord".into(), opacity: 0.9, ..Appearance::default() };
        assert!(palette(&nord).contains("surface rgba(46, 52, 64, 0.90)"));
    }
}
