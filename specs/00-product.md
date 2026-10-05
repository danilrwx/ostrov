# 00 — The product

## What

ostrov ("island") is a whole desktop shell for Hyprland in one Rust binary: the bar, its panels, the launcher,
notifications, the lock screen, the polkit agent, askpass and the rest of what a Hyprland desktop is usually pieced
together from, made to look and work as one thing. The name is the idea: everything of the shell in one place
(2026-10-03, «ведь идея изначально острова в этом и была»).

## For whom

- First, its owner: a laptop (HONOR MagicBook Pro 14, 3120×2080 at 2×) on Ubuntu, Hyprland, a keyboard-first
  user coming from i3 and dwm, gaming on Steam.
- Then anyone: installed on a bare Hyprland it is a good-looking, working desktop without configuring anything
  (2026-10-04).

## Principles

| ID | Requirement | Status | Where / gap |
|---|---|---|---|
| P-1 | ostrov SHALL be one binary that replaces the bar, notifications, OSD, launcher, clipboard manager, lock, idle, wallpaper, polkit agent and askpass of a Hyprland desktop (2026-10-03: «polkit, swaybg, swayidle ... можно ли их альтернативы в остров засунуть», cliphist, the screenshot tool) | ✅ | `src/main.rs`; swaybg, swayidle, swaylock, cliphist, dunst, grim's UI, lxpolkit no longer needed |
| P-2 | WHEN installed on a bare Hyprland, ostrov SHALL look good and work without any configuration; what it needs from hyprland.conf SHALL be recommendations, not requirements (2026-10-04) | ✅ | ostrov adds its own layer rules and binds on free keys (`src/modules/hyprland.rs`), `ostrov doctor` tells the rest |
| P-3 | ostrov SHALL have excellent defaults: every config key has one, no file needed to start | ✅ | `src/config.rs`, `config/example.toml` checked against the defaults by a test |
| P-4 | ostrov's look SHALL be contrasting, a little rounded, "brutal" rather than soft: black, white, a thin accent (2026-10-03: «грубоватый брутальный и контрастный») | ✅ | `src/style.rs`, the dark theme |
| P-5 | ostrov's animations SHALL be short (about 100 ms) and few; nothing jumping, nothing jerking when something opens (2026-10-03: «столько анимаций, для меня это ужас», «панель дергается») | ✅ | `src/popup.rs`; `[appearance] animations = false` turns them off |
| P-6 | ostrov SHALL be modular inside: a part of the desktop is one module (state, commands, worker, widgets), the rest of ostrov not naming it (2026-10-04) | ✅ | `src/modules/mod.rs` |
| P-7 | ostrov SHALL be public, open, extensible by others: plugins in any language, widgets without code, themes, an API (2026-10-04) | ✅ | see 08-extensibility.md; publishing itself see 10 |
| P-8 | ostrov SHALL use little memory: about the i3 desktop's footprint as the goal (2026-10-05: «до уровня системы на и3 реально ужаться?») | 🟡 | ~52 MB PSS at start (from ~308 MB): glibc arenas capped, lavapipe not loaded (`src/hub.rs` `lean()`). Further cuts (popups as separate processes) were tried and dropped (decisions.md) |
| P-9 | ostrov SHALL depend on as few outside programs as it can; what it needs SHALL be listed per distribution (2026-10-04: drives «без доп утилит?», «если ставить на арч ... добавить в требования») | ✅ | README Requirements; `ostrov doctor` |
| P-10 | ostrov SHALL speak English and Russian, the locale's language by default (2026-10-04) | ✅ | `src/i18n.rs`, `i18n/ru/` |
| P-11 | ostrov SHALL target Hyprland only; other compositors are out of scope until asked again (2026-10-05: X11, sway, dwl, own compositor tried and dropped) | ✅ | sway's IPC removed (2026-10-05); under another compositor the bar still runs without Hyprland's parts (docs/requirements.md) |
| P-12 | ostrov SHALL feel familiar to a macOS and GNOME user where that helps: a control centre of widgets, a Spotlight-like launcher, Night Shift, screenshots, a clock in the middle (2026-10-03) | ✅ | the bar's default layout, panels |
| P-13 | WHERE the owner works, ostrov SHALL keep the i3 habits: workspaces at the left like i3's, the bar hidden and shown while Super is held (2026-10-01, 2026-10-03) | ✅ | `[bar] left = ["workspaces", ...]`; peek in `src/bar/mod.rs` |

## Out of scope

- Being a compositor, a window manager, an overview or a window switcher: those are the compositor's (decisions.md).
- A login screen of its own (decisions.md).
- The system's settings app: displays in depth, users, printers. ostrov does the quick settings; deep ones open
  their programs.
