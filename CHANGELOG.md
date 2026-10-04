# Changelog

## 0.1.0

The first release: the whole shell in one binary.

- The bar, made of blocks: workspaces with animated dots, the focused window's title, the keyboard layout, the
  system tray with its menus, the privacy indicator, the recording's timer, panels' faces made of their widgets'
  badges. Docked or hidden and peeking (`ostrov bar toggle|peek|unpeek`); the blocks set in `[bar]` or with the
  bar's editor in Settings.
- Panels: the control centre and the calendar as grids of widgets eight cells wide, unrolling out of the bar; Edit
  drags, resizes by a corner, removes and adds widgets from a gallery, an inspector per widget (sizes, its badge in
  the bar always, while active or never, the bar alone, its settings); panels of your own (`[panels.ID]`).
- Widgets: Wi-Fi (iwd) with its networks and passphrases, Bluetooth with pairing, volume and mic with per-app
  sliders and devices, outputs in a card's other profiles switched to, a Bluetooth headset's profile, brightness,
  power profiles, battery, night light (on, by the clock or sunset to sunrise), displays (modes, scale, position,
  mirroring, saved profiles), wallpaper, Keep Awake, screenshot, lock and power off, the player (MPRIS), the
  clock, the month with CalDAV and .ics events, upcoming events, the weather (open-meteo), the notifications'
  history with Do Not Disturb.
- The launcher in the bar: apps, a calculator, emoji, files (fd), web search (DuckDuckGo, `[launcher] search`),
  the clipboard's history with previews; plugins' modes (`[[launcher]]`: `?question` to Claude and `g words` to
  Google in the examples).
- Notifications server with toasts and an OSD; the media and Fn keys (`ostrov key NAME`) shown in the OSD;
  battery warnings.
- The lock screen through ext-session-lock and PAM, surviving a restart of ostrov; idle handling through
  ext-idle-notify (lock, screens off, lock before sleep, Keep Awake).
- The polkit agent and an SSH askpass in ostrov's own dialogs; `ostrov dialog JSON` for scripts.
- The clipboard's history through wlr-data-control, text and pictures, password managers' entries left out.
- Screenshots of a region or the screen through wlr-screencopy; screen recording through GStreamer (VA-API or
  openh264, optional audio).
- The wallpaper, picked from a directory, with a hook run on every change.
- Games played in the performance power profile (`[games]`).
- Settings and Appearance pages in the control centre, editing `config.toml` in place; themes (dark, light,
  graphite, nord, solarized), accent, opacity, corners, density, Hyprland's blur.
- Themes as packages: `theme.toml` (the palette's colours, suggested corners, density, blur) and an optional
  `theme.css`, installed from a path or a git URL (`ostrov theme list|set|install|remove`), on the Appearance page
  beside the built-in five, which are the same format (docs/themes.md).
- At home in a bare Hyprland: its layer rules, the lock's takeover and its keys put in over IPC where free
  (`[hyprland]`, `ostrov hyprland`); `ostrov doctor` checks the system.
- Extensibility: widgets in KDL (`~/.config/ostrov/widgets/*.kdl`), plugins in any language over JSON lines
  (`wit/ostrov-plugin.wit`, a Python SDK), following ostrov's events, binding keys where free, adding calendars,
  installed and removed while ostrov runs (`ostrov plugin install PATH|GIT-URL`, `remove ID`); a D-Bus interface
  (`dev.ostrov.Shell`).
- Shell completion for zsh, bash and fish, answered by the running ostrov.
