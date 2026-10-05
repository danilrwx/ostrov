# 04 — Launcher, clipboard, screenshots

| ID | Requirement | Status | Where / gap |
|---|---|---|---|
| RUN-1 | The launcher SHALL be inside the bar, dmenu-style, not a window of its own (2026-10-03: «dmenu внутри бара») | ✅ | `src/launcher.rs` |
| RUN-2 | Super+D SHALL open it; WHEN a click lands outside, it SHALL close (2026-10-03) | ✅ | `src/modules/hyprland.rs` KEYS, `src/popup.rs` |
| RUN-3 | It SHALL search case-insensitively and list only the apps that can run (2026-10-01, 2026-10-02) | ✅ | `src/launcher.rs` |
| RUN-4 | It SHALL have modes: apps, a calculator, emoji (`:name`), files (`/name`), web search (`s words`), engines of the user's own by prefix (`g words`, `?question` to Claude in the browser) (2026-10-03, 2026-10-04) | ✅ | `src/launcher.rs`, `src/calc.rs`, `[launcher] engines` |
| RUN-5 | The web search SHALL default to DuckDuckGo; Google SHALL be an engine the user adds (2026-10-04) | ✅ | `[launcher] search` |
| RUN-6 | Terminal apps (htop, nvim) SHALL open in `$TERMINAL`, not a hard-coded one (2026-10-04) | ✅ | `src/launcher.rs` |
| RUN-7 | Plugins SHALL add launcher modes (2026-10-04) | ✅ | `docs/plugins.md` Launcher modes |
| RUN-8 | Ctrl+N/P SHALL move the pick in every list: the launcher, the clipboard (2026-10-03) | ✅ | `src/launcher.rs` |
| CLIP-1 | ostrov SHALL keep the clipboard's history itself, text and pictures, without cliphist (2026-10-03) | ✅ | `src/clip.rs` |
| CLIP-2 | Its list SHALL tell pictures from text and show their content (2026-10-03) | ✅ | `src/clip.rs` |
| CLIP-3 | Password managers' entries SHALL be left out of the history | ✅ | `src/clip.rs` |
| SHOT-1 | ostrov SHALL take screenshots of a region or the screen into the clipboard, without a separate tool (2026-10-03) | ✅ | `src/shot.rs` (grim's protocol, no grim) |
| SHOT-2 | A whole-screen shot SHALL take one click, without moving the pointer first (2026-10-03) | ✅ | `src/shot.rs` |
| SHOT-3 | A colour picker (a pixel's hex into the clipboard) | 🚫 | not picked from the 2026-10-03 list |
