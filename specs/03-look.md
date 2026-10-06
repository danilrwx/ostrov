# 03 — The look

| ID | Requirement | Status | Where / gap |
|---|---|---|---|
| LOOK-1 | ostrov SHALL ship themes (dark, light, graphite, nord, solarized) and an accent; the default the dark one, black (2026-10-04) | ✅ | `themes/`, `src/look.rs` |
| LOOK-2 | Appearance SHALL be a page in the UI: theme, accent, surface colour, opacity, bar colour, blur, font, sizes, animations, writing `config.toml` in place, comments kept, applied as it is saved (2026-10-04) | ✅ | `src/cc/appearance.rs`, `src/settings/store.rs` |
| LOOK-3 | The surface's colour (under everything that opens) SHALL come from the theme and be changeable by the user (2026-10-04) | ✅ | `[appearance] surface` |
| LOOK-4 | Built-in themes SHALL keep their surface and raised colours apart enough to read without transparency, not too contrasting either (2026-10-04) | ✅ | `themes/*/theme.toml` |
| LOOK-5 | A hovered block or row SHALL take the surface's colour, the same as the open panel's (2026-10-03, 2026-10-04: «hover должен быть цветом surface») | ✅ | `src/style.rs` |
| LOOK-6 | The bar, its blocks, the open panel and its tab SHALL be one colour where they meet: no seam, no border of another colour (2026-10-04) | ✅ | `src/style.rs`, `src/popup.rs` |
| LOOK-7 | Blur SHALL be independent of transparency: a nearly clear panel with strong blur is possible (2026-10-04) | ✅ | `[appearance] blur`, Hyprland's layer rules set by ostrov (`src/modules/hyprland.rs`) |
| LOOK-8 | IF the bar is opaque, or blur is off, THEN hovering SHALL NOT flicker, and colours SHALL match as with blur (2026-10-04: «мигает как ёлка», «если блюр выключить то всегда криво») | ✅ | dynamic `ignore_alpha` under the opacity, blur off when disabled |
| LOOK-9 | How the open panel's tab meets the bar SHALL be a setting (2026-10-04: «можем на это поведение настройку сделать?») | ✅ | the tab ground setting, `src/look.rs` |
| LOOK-10 | Greys SHALL read the same everywhere: the calendar's dim numbers as the weather's (2026-10-03) | ✅ | colour tokens in `src/style.rs` |
| LOOK-11 | Panels on an accent SHALL not use the plain grey for secondary text (2026-10-04) | ✅ | `src/style.rs` |
| LOOK-12 | Themes SHALL be installable packages: a palette and optional CSS, `ostrov theme install PATH|GIT-URL` (2026-10-04) | ✅ | `docs/themes.md` |
| LOOK-13 | The other apps SHALL follow ostrov's look: the scheme and the accent through GNOME's settings and the portal (GTK, Qt, Telegram, Chrome), GTK's palette, colour files and templates for terminals and Telegram (2026-09-29 dropped, 2026-10-06 taken up again: «1-3 делаем») | ✅ | `src/apps.rs`, `[appearance] apps`, docs/themes.md |
| LOOK-14 | Fonts and icons in the bar SHALL be small and in the text's style; the screen's rounded corners SHALL not cut the bar's ends (2026-10-01) | ✅ | `bar_icon_size`, `bar_padding` |
| LOOK-15 | ostrov's blur and layer rules SHALL take effect: Hyprland 0.53 keeps no rule given by keyword, so they are written to ~/.local/state/ostrov/hyprland.conf, which hyprland.conf sources, Hyprland reloaded as they change and the bar mapped again (2026-10-06) | ✅ | `hyprland::put_rules`, `popup::remap_when_closed`, doctor |
| LOOK-16 | The bar SHALL be blurred or not, the panels blurred either way: unblurred, its dark ground on a layer of its own under it, no dark rim under the strip that Hyprland's blur leaves at its edge (2026-10-06) | ✅ | `[appearance] blur_bar`, `popup::ground`, `window.grounded` |
| LOOK-17 | The other apps SHALL be integrations of their own: an app.toml and a template each, built in for the popular ones, the user's own in ~/.config/ostrov/apps, each connected or not (its config made to read ostrov's file, the line taken out again), told to read it again as it changes (2026-10-06: «база и расширяемость») | ✅ | `src/integrations.rs`, `apps/`, `ostrov apps`, docs/apps.md |
