# Modules and widgets

Every part of the desktop ostrov has, what it reads, its commands (`ostrov ID ARGS`), its widgets (their ids for
`panel.toml` and the gallery, the sizes they allow in cells of the 8-wide grid, the first the default) and its
settings. Every key has a default; `config/example.toml` lists them all with comments. `ostrov dump` prints the
state each module keeps, under its id.

## Sound — `audio`

PipeWire through `wpctl`, `pw-dump` and `pw-cli`: the outputs and inputs, a card's ports and its other profiles'
outputs (a click switches the profile), the apps playing each with its own slider, what uses the mic, the camera
or the screen.

| Widget | Sizes | What |
|---|---|---|
| `volume` | 8×1 | the output's slider, its menu the outputs (under an arrow) and the apps playing |
| `mic` | 8×1 | the input's slider and mute, its menu the inputs |
| `headset` | toggle sizes | a Bluetooth headset's mode, handsfree (with its mic) or headphones, there only while one is connected |

Commands: `ostrov audio ...` (the privacy menu's mute, stop-camera, stop-screen among them), `ostrov key
vol-up|vol-down|vol-mute|mic` (the keys, 2% a step, with the OSD).

## Wi-Fi — `wifi`

iwd, else NetworkManager, over D-Bus. Its toggle's menu lists the networks; one asking for a passphrase opens its
field under itself. Commands: `ostrov wifi on|off|scan|disconnect`, `connect SSID`, `forget SSID`.

## Airplane Mode — `airplane`

rfkill: every radio soft-blocked. `ostrov airplane on|off|toggle`; its toggle `airplane`.

## Bluetooth — `bt`

BlueZ over D-Bus: power, scanning, pairing, connecting; connected devices' batteries.

| Widget | What |
|---|---|
| `bt` | the toggle, its menu the devices |
| `bt-battery` | the connected devices' charge (headphones, mice) |

Commands: `ostrov bt on|off|scan`, `connect|disconnect|pair|forget ADDR`.

## Power Mode — `power`

power-profiles-daemon: power-saver, balanced, performance. `ostrov power set PROFILE`; its toggle `power`.

## Battery — `battery`

UPower's display device: the charge, charging or not, the time left (in the tooltip). The charge limit writes the
battery's `charge_control_end_threshold`, root's until `ostrov battery limit install` puts a udev rule letting the
user write it.

| Widget | Sizes | What |
|---|---|---|
| `battery` | 5×1, 2×1, 3×1, 4×1, 8×1 | its icon and percent, from three cells wide the time left |
| `charge` | toggle sizes | the limit: 80% or full, its menu 60/80/90/100 |

Commands: `ostrov battery limit PERCENT`, `ostrov battery limit install`. Warnings at 20% and 10% while
discharging, at 5% a question to suspend.

## Brightness — `brightness`

logind's SetBrightness, no brightnessctl. `ostrov brightness PERCENT`, `ostrov key bright-up|bright-down` (5% a
step, the OSD); its slider `brightness` (8×1).

## Keyboard layout — `keymap`

Hyprland's main keyboard's layouts (`input:kb_layout`, us,ru) and the active one, kicked on every switch.
`ostrov keymap next`, `ostrov keymap set N` (0 the first). Its widget `keymap` (1×1 to the whole width): the layout
in the icon's place, its name beside it, a click the next layout, its menu every layout; in the bar by itself as
`widget.keymap` (the `layout` block is it), a left click the next layout, a right click the menu.
`[widget.keymap] show` says how the layout shows: `language` (EN, RU, the default), `flag` (🇺🇸 🇷🇺) or `code`
(us, ru).

## Now Playing — `media`

MPRIS: the player playing (or the last one that did), its art, title, artist and controls. `ostrov media
play-pause|next|previous`, the media keys too; its widget `media`.

## Calendar — `calendar`

The month and what is coming up, the events from plugins that are calendars (the official `caldav`: CalDAV
accounts and .ics links). `ostrov calendar refresh`.

| Widget | What |
|---|---|
| `month` | the month, today marked, days with events dotted |
| `agenda` | the events coming up |

## Clock — `clock`

The time large over the date; its badge in the bar the time as `[widget.clock] format` says (strftime's, GNOME's
`%a %b %-d  %H:%M` by default).

## Weather — `weather`

open-meteo every quarter of an hour, for the location set once by `ostrov location CITY` (open-meteo's
geocoding) or `ostrov location LAT LON`, kept in `~/.local/state/ostrov/location.json`. Its widget `weather`, its badge the temperature and the sky.

## Wallpaper — `wallpaper`

ostrov draws the wallpaper itself, under everything, black until a picture is picked. `ostrov wallpaper
on|off|random`, `set PATH`; its toggle `wallpaper`, its menu the pictures of `[widget.wallpaper] dir`.
`on_change` runs a command after every pick, the picture in `$OSTROV_WALLPAPER` (to theme a terminal from it).
`bar` sets how solid the bar is over a picture.

## System — `system`

| Widget | What |
|---|---|
| `screenshot` | a region into the clipboard (`ostrov screenshot`) |
| `lock` | the lock (`loginctl lock-session`) |
| `session` | the power menu: suspend; restart, power off and log out asked first |
| `awake` | Keep Awake: idle neither locks nor turns the screens off |
| `notifications` | the history, Do Not Disturb, Clear |

## System monitor — `sysinfo`

The processor's load, the memory in use, the processor's temperature (coretemp, k10temp, zenpower, else ACPI's
zone) and the network's speed down and up, from `/proc` and `/sys` every two seconds. Its widget `sysinfo`: two
cells the load and memory, four the temperature too, eight the network as well; its badge the load and memory.

## Hyprland — `hyprland`

ostrov's integration: the layer rules its surfaces need (blur under the bar and the panels) and its keys, added
where free unless `[hyprland] rules`/`binds` say no; `[hyprland.keys]` moves one (`run = "SUPER, R"`).
`ostrov hyprland` prints the lines it would add, for a config of your own.

| Key | Does |
|---|---|
| Super+D | the launcher |
| Super+X | the control centre |
| Super+C | the calendar |
| Super+Shift+V | the clipboard's history |
| Super+Shift+S, Print | a screenshot of a region |
| Super+Shift+X | lock |
| Super+B | the bar hidden or shown; held Super shows it while hidden |
| the media, volume, brightness and mic keys | with the OSD |

## Outside the modules

- **The bar's editor** (`ostrov bar edit`, or Bar in Settings): the bar's parts outlined and a gallery of the
  blocks not in it under the bar; drag a block into a part, along it or off the bar, Tab, Shift+arrows and Delete
  from the keyboard; Done writes `[bar]` and every bar follows at once, Cancel or Escape puts it back.
- **Launcher** (`ostrov run`): apps, arithmetic calculated as typed, `:` emoji, `/` files, `s ` a web search
  (`[launcher] search`, DuckDuckGo by default), engines by prefix (`[launcher] engines`), plugins' modes.
- **Clipboard** (`ostrov clip`): text and pictures, password managers' entries left out.
- **Notifications**: the server, toasts, the history, quiet (`[notifications]`).
- **Lock and idle** (`[idle] lock`, `screens_off`, seconds).
- **Polkit agent** (`[polkit] agent`), **askpass** (`ostrov askpass`), **share picker** (`ostrov-share-picker`).
