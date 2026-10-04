# displays

ostrov's official plugin for the screens, as Windows' Win+P: a widget whose menu sets

- **Laptop screen only**: the laptop's own (eDP, LVDS, DSI; the first monitor where there is none), the others off;
- **External screen only**: the others on, the laptop's off;
- **Extend**: all on, the external ones beside the laptop's on the side `side` says;
- **Mirror**: the external ones showing the laptop's picture.

Each is a few `hyprctl keyword monitor` rules, the monitors read from `hyprctl -j monitors all`; a monitor already
on keeps its mode and scale, one turned on gets its preferred mode. Like any `hyprctl keyword`, the rules last
until Hyprland reloads its config. The toggle itself extends (on) or goes back to the laptop's screen (off).

While [kanshi](https://gitlab.freedesktop.org/emersion/kanshi) runs, its profiles (`profile NAME {` in
`~/.config/kanshi/config`) are listed under the modes, a click switching to one with `kanshictl switch NAME`.
Layouts kept by name and applied as monitors are plugged in are kanshi's work, not ostrov's.

```toml
[plugin.displays]
enabled = true
side = "right"        # where Extend puts the external screen: right, left, above, below
```

Then **Edit** in the control centre, and pick "Displays" from the gallery (after `ostrov restart`).

    ostrov plugin displays menu                                the control centre, its menu unfolded
    ostrov plugin displays mode laptop|external|extend|mirror  the screens set so
    ostrov plugin displays profile NAME                        kanshi's profile switched to

Keys (permission `keys`, bound where free): `XF86Display` and `Super+P` (what a laptop's projection key sends)
open its menu.

## From ostrov's built-in Displays

The widget, its keys and `ostrov displays ...` were ostrov's own before; they are this plugin's now:

- turn it on (above) and put its widget on the grid again;
- `ostrov displays on|off|mirror|set ...` are gone: a mode with `ostrov plugin displays mode ...`, a monitor's
  own mode, scale and place in hyprland.conf's `monitor =` lines or a kanshi profile;
- the profiles of `~/.config/ostrov/displays.toml` are not read any more: write each as a kanshi profile, and
  run kanshi from hyprland.conf (`exec-once = kanshi`). A profile such as

  ```toml
  [[desk.monitors]]
  description = "Dell Inc. DELL U2720Q 1234ABC"
  enabled = true
  mode = "3840x2160@60.00"
  position = "1560x0"
  scale = 1.5

  [[desk.monitors]]
  description = "EDO EDO14.55"
  enabled = false
  ```

  is in `~/.config/kanshi/config`

  ```
  profile desk {
      output "Dell Inc. DELL U2720Q 1234ABC" mode 3840x2160@60Hz position 1560,0 scale 1.5
      output "EDO EDO14.55" disable
  }
  ```

  kanshi applies it by itself whenever exactly those monitors are connected (`transform = 1` is `transform 90`).
  kanshi has no mirroring; a profile's `exec hyprctl keyword monitor NAME,preferred,auto,1,mirror,eDP-1` line
  does it.

Then `displays.toml` can go.
