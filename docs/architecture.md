# Architecture

One process, `ostrov`, a GTK 4 application (`dev.ostrov.Ostrov`): `ostrov ARGS` run again hands ARGS to the one
running and exits. Two threads of work: GTK's main thread draws everything; a Tokio runtime (`services/`) does
everything that waits — D-Bus, processes, files, the network. A plugin is a process of its own.

```
             ┌──────────── Tokio runtime (services/) ────────────┐
 D-Bus,      │ each module's worker (modules/*/service.rs)       │
 /sys, procs │   kicks ─► its state read ─► one JSON state ──────┼──► hub.rs (GTK thread)
 ──────────► │ commands: `ostrov wifi connect X` ─► module.run   │       │ every new state handed
             └───────────────────────────────────────────────────┘       ▼ to whoever asked
                                                                   bar/, cc/ (panels, widgets),
   compositor ── wm.rs (Hyprland's sockets) ── bar events          plugins/, widgets/ (KDL), events.rs
```

## The pieces

| Where | What |
|---|---|
| `main.rs` | start-up (what is built, in order), and the one dispatch of `ostrov ARGS` (`FORMS`, the grammar of `forms.rs`) |
| `services/` | the runtime, the buses, the state put together from every module's and sent on as it changes, helpers (D-Bus, rfkill, location, local time) |
| `modules/` | a part of the desktop whole: `Module { id, forms, state, run, worker, widgets }` in `ALL`; nothing else names one |
| `hub.rs` | the state on GTK's side: the last one kept, handed to each part that asked; the services' commands |
| `wm.rs` | Hyprland: workspaces, its event socket, screens on and off, its focus grab for popups |
| `bars.rs`, `bar/` | a bar per monitor; the bar's blocks by name (workspaces, window, layout, privacy, tray, `panel.ID`) |
| `popup.rs` | the bar's window and what unrolls out of it: one surface, the block a tab, the popup under it |
| `cc/` | panels: the grid (`grid.rs`, 8 wide, widgets as rectangles), Edit, the gallery, the inspector, the bar editor, Appearance |
| `ui.rs` | the kit: toggle, slider, round button, menu card and rows, chips; the Settings' controls |
| `style.rs`, `look.rs`, `theme.rs` | every colour a named token; the theme's palette, accent, surface, density, radii; Hyprland's blur |
| `settings/` | schemas (sections of fields), the forms drawn from them, `config.toml` edited in place (`store.rs`), secrets |
| `notes.rs`, `keys.rs` | the notification server, toasts and OSD; the media and Fn keys, battery warnings |
| `launcher.rs`, `clip.rs`, `calc.rs` | the launcher in the bar, its modes; the clipboard's history |
| `lock.rs`, `idle.rs`, `polkit.rs`, `prompt.rs`, `share.rs` | lock, idle, the polkit agent, the kit's dialog (askpass, polkit, plugins), the share picker |
| `shot.rs`, `wallpaper.rs` | screenshots, the wallpaper |
| `plugins/` | the host: manifests, processes (`process.rs`), their node trees drawn (`node.rs`), install, the catalogue |
| `widgets/` | KDL widgets: sources (poll, listen, event), expressions, drawn with the plugins' renderer |
| `events.rs`, `api.rs` | ostrov's events (window, workspace, lock, network...); its D-Bus face `dev.ostrov.Shell` |
| `doctor.rs`, `welcome.rs` | the checks; the first run |
| `i18n.rs` | `t()`, `fill()`, `plural()`; catalogues in `i18n/LANG/*.toml` |

## How a change flows

1. A module's worker hears something (a D-Bus signal, a udev event, a timer) and kicks.
2. The runtime reads that module's state and, if the whole state changed, sends it to GTK's thread.
3. `hub.rs` hands it to each part; a widget's `draw` reads its module's key and updates its labels.
4. A click runs a command (`hub::service(&["wifi", "connect", ssid])`), which goes back to the module's `run`
   on the runtime; its worker's next kick brings the new state.

A plugin goes the same way from the other side: ostrov sends it the state and its config, it answers `render`
with a node tree, pushes new trees when it likes (`kick`), and its nodes' events come back to `on_event`.

## Rules the code keeps

- GTK on its thread only; anything that blocks goes to the runtime or a thread of its own.
- No `unwrap()` on paths that run; errors to the user (a toast, a dialog) or stderr as `ostrov: ...`.
- Colours only as tokens from `style.rs`.
- A module, widget or plugin added touches its own files and one list, nothing else.
- Tests beside the code (`#[cfg(test)]`), `cargo test --release`, no warnings.
