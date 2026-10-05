# Decisions

What was tried or proposed and decided against. Not to be proposed again without a new reason.

| Date | Decision | Why |
|---|---|---|
| 2026-09-29 | One theme switch for the whole desktop (terminal, Chrome, k9s, nvim, transparency everywhere) dropped | the owner wanted just the dark theme back |
| 2026-09-29 | Noctalia, ashell and the like not used instead of an own shell | «мой изначальный вариант лучше всех» |
| 2026-10-03 | Quickshell (QML) replaced by ostrov in Rust | fewer dependencies, the owner's own design |
| 2026-10-03 | Plugins as WebAssembly later; JSON-lines processes now | any language today, a sandbox to come |
| 2026-10-04 | No window switcher, no overview (Mission Control) in ostrov | the compositor's job (hyprexpo and the like), a separate utility if ever |
| 2026-10-04 | No login screen of ostrov's (greeter) | «получилось не очень»; tuigreet or regreet do it |
| 2026-10-04 | Applying the user's style to the login screen dropped | too much for a shell |
| 2026-10-04 | A colour picker not taken | not picked from the list |
| 2026-10-04 | Monitoring and weather "not now" (2026-10-03), later the weather put in the core, monitoring as the System widget (2026-10-05) | changed mind as the panels got widgets |
| 2026-10-05 | ostrov on X11/i3 (branch x11, eww, picom) dropped, i3 removed from the desktop | panels' interactivity and animations far worse, scale broken |
| 2026-10-05 | Memory cut by splitting popups into processes dropped | «давай лучше оставим как было» |
| 2026-10-05 | dwl as the desktop with ostrov-ctl's status line dropped | «не вижу смысла инвестировать токены в это» |
| 2026-10-05 | An own compositor (stopka: Rust/smithay, then C on dwl) dropped | memory and effort; Hyprland stays |
| 2026-10-05 | Umbriel + Noctalia tried and removed | not wanted |
| 2026-10-05 | sway instead of Hyprland not done | Steam blurry at 2× on sway (no zero-scaling for Xwayland), tearing and scroll already fine on Hyprland |
| 2026-10-05 | Waybar-format script blocks not added | KDL `listen` already covers them |
| 2026-10-05 | Rewriting ostrov in another language (TypeScript+Astal, Python, C) not decided | the cost of a rewrite against faster builds in Rust; open if build times or verbosity hurt |
