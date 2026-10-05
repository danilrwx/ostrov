# Troubleshooting

Start with `ostrov doctor`: a line for each thing ostrov works with, `✓` there, `!` missing and what that costs,
`·` for information. ostrov's own log is its stderr; started from Hyprland's `exec-once`, it is wherever that
sends it (`exec-once = ostrov 2>>~/.local/state/ostrov/log` keeps it).

## What doctor says

| Line | Means | Do |
|---|---|---|
| `! Hyprland: not running` | another compositor | workspaces, the window's title and the layout block need Hyprland (docs/requirements.md) |
| `· blur is off in Hyprland` | `decoration:blur:enabled = false` | the panels are see-through without blur; turn it on, or raise `[appearance] opacity` |
| `! SUPER, D is <something>, not ostrov run` | the key is taken in hyprland.conf | free it, or move ostrov's: `[hyprland.keys] run = "SUPER, R"` |
| `· SUPER, D: ostrov run (bound at the next start)` | ostrov binds it when it starts | `ostrov restart` |
| `! Wi-Fi: neither iwd nor NetworkManager` | no Wi-Fi service | install and start one |
| `! … missing: …` (BlueZ, UPower, power-profiles-daemon, logind, polkit) | the service is not on the system bus | install it (docs/requirements.md) |
| `· keyring: no Secret Service` | no gnome-keyring or the like | secrets go to a 0600 file; install one to keep them in a keyring |
| `! wpctl missing` | no PipeWire tools | install `pipewire-bin` (Debian) / `pipewire` |
| `· charge limit: the thresholds are root's` | the battery's threshold is not the user's | `ostrov battery limit install` (asks for the password once) |
| `· polkit agent: lxpolkit, in its own look` | another agent answers first | its autostart is kept out of Hyprland; ostrov answers from the next login (`ostrov welcome` explains) |
| `! polkit agent: none` | polkit is not running | install `polkitd` |
| `· screen sharing: xdph's own picker` | xdph does not ask ostrov | the line doctor prints, in `~/.config/hypr/xdph.conf` |
| `· PAM: no /etc/pam.d/ostrov` | the lock checks as `login` does (a fingerprint reader may make it wait) | install the package's profile, or copy `packaging/pam/ostrov` |

## Common things

**Notifications do not show.** Another notification daemon (mako, dunst, swaync) owns
`org.freedesktop.Notifications`. Stop it and remove it from your autostart; ostrov takes the name as it starts.

**A plugin's widget is not in the gallery.** Widgets join the gallery when ostrov starts: `ostrov restart` after
installing or turning on a plugin. `ostrov plugins` says what was found and whether each is enabled; a manifest
that does not read is said on stderr.

**A KDL widget does nothing.** A file that does not read is said in a toast and on stderr with its line. A widget
new to a file waits for `ostrov restart`; one edited redraws at once.

**The bar flickers when hovered, or colours differ between the bar and a panel.** Blur's `ignore_alpha` must be
under the bar's opacity; ostrov sets its own layer rules for that. A `layerrule` of yours for the namespace
`ostrov` overrides them: remove it, or `[hyprland] rules = false` and keep yours whole.

**Keys do nothing.** `ostrov doctor` lists every key, bound to ostrov or taken. `[hyprland] binds = false`
means ostrov binds none: put `ostrov hyprland`'s lines in hyprland.conf.

**The lock screen waits after the password.** The system's `login` PAM stack has fprintd; install
`/etc/pam.d/ostrov` (password only).

**Steam, games or X11 apps are blurry at 2×.** Not ostrov's: Hyprland's `xwayland { force_zero_scaling = true }`
and the app's own scale (`STEAM_FORCE_DESKTOPUI_SCALING=2`).

**ostrov uses more memory than expected.** `ostrov dump` and the process's PSS (`smem`, `/proc/PID/smaps_rollup`)
tell. ostrov caps glibc's arenas and keeps Mesa's software Vulkan out; a plugin is a process of its own, counted
apart.

## Reporting a bug

Say what you did, what you expected and what happened; attach `ostrov doctor`, ostrov's stderr around it, and
`hyprctl version`. Nothing personal is in either but your Wi-Fi's and devices' names: look before you paste.
