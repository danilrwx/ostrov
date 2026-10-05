# 08 — Extensibility

The owner's words (2026-10-03, 2026-10-04): plugins are «бинарные модули, которые можно писать на разных языках,
аля webasm»; «при подключении плагина создаётся внутри острова сабкоманда с API плагина»; «каждый виджет включая
кастомные тоже должен иметь API для настройки, допустим плагин требует токен».

What belongs to the core and what to extensions (agreed 2026-10-04): the core has what needs low-level access or
privileges (Wayland protocols, PAM, the session's lock, system-bus agents), what every desktop has through standard
Linux services (PipeWire, BlueZ, UPower, logind, power-profiles-daemon), and the frame extensions stand on (panels,
UI kit, settings, API). Extensions have what is tied to one provider or service, to taste, to rare hardware, or
goes to someone's API on the internet.

| ID | Requirement | Status | Where / gap |
|---|---|---|---|
| EXT-1 | A plugin SHALL be a program in any language speaking ostrov's protocol (JSON lines over a process today, WebAssembly later), installed with `ostrov plugin install PATH|GIT-URL` after showing its permissions (2026-10-03, 2026-10-04) | 🟡 | `src/plugins/`, `wit/ostrov-plugin.wit`; the WebAssembly backend is not done |
| EXT-2 | A plugin's commands SHALL be `ostrov plugin ID ARGS`, stdin passed, the answer on stdout, listed in `ostrov help` (2026-10-04) | ✅ | `src/plugins/mod.rs` |
| EXT-3 | A plugin SHALL put widgets in panels, made of the UI kit, updated in place (2026-10-04) | ✅ | `docs/plugins.md` Nodes |
| EXT-4 | A plugin's settings SHALL be fields on the Settings page and in its widget's inspector, a token kept in the Secret Service (2026-10-04) | ✅ | `src/settings/` |
| EXT-5 | A plugin SHALL follow ostrov's events (the wallpaper, the lock, the workspace, the network) (2026-10-04) | ✅ | `src/events.rs` |
| EXT-6 | A plugin SHALL declare keys, bound in Hyprland where they are free (2026-10-04) | ✅ | `[[keys]]` in the manifest |
| EXT-7 | A plugin SHALL be a calendar (its events merged into the calendar) (2026-10-04) | ✅ | `calendar = true` |
| EXT-8 | A plugin SHALL add launcher modes (2026-10-04) | ✅ | |
| EXT-9 | A plugin SHALL ask the user through ostrov's dialogs (2026-10-04) | ✅ | |
| EXT-10 | A plugin SHALL put a block of its own in the bar | ❌ | gap (same as BAR-18) |
| EXT-11 | Official plugins SHALL come from a catalogue: `ostrov plugin install NAME` (2026-10-04's API list) | ❌ | gap: installed from the repo's `plugins/` by the owner's install script; no catalogue |
| EXT-12 | SDKs SHALL exist for Rust and Python (2026-10-04) | ✅ | `sdk/`, `sdk/python` |
| EXT-13 | Widgets SHALL be writable without code, in KDL: commands polled or listened to, expressions, toggles, sliders, menus, badges (2026-10-04) | ✅ | `src/widgets/`, `docs/widgets.md` |
| EXT-14 | Scripts SHALL reach ostrov over D-Bus (`dev.ostrov.Shell`: Run, State, StateChanged, OpenPanel, Toast) and its commands (`ostrov ID ARGS`) (2026-10-04) | ✅ | `src/api.rs` |
| EXT-15 | Every command SHALL complete in zsh, bash and fish, completing SSIDs, devices, widgets, settings and plugins' commands from the running ostrov (2026-10-05: «completions сделал?») | ✅ | `completions/`, `src/forms.rs` |
| EXT-16 | Waybar-format scripts (JSON `text`/`alt`/`tooltip`) as blocks | 🚫 | 2026-10-05: «не уверен зачем он нам»; KDL `listen` covers it |
| EXT-17 | A headless `ostrov-ctl` (services without GTK) SHALL give a status line and wmenu menus for a desktop without ostrov's GTK (2026-10-05, for dwl) | ❓ | built (`src/bin/ostrov-ctl/`), but dwl was dropped the same day; keep it, or remove it and its README section |
| EXT-18 | Settings could be done by separate small utilities instead of the shell ("a settings manager") (2026-10-05) | ❓ | idea parked by the owner: «подвесить в воздухе и реализовать позже» |
