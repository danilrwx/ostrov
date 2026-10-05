# What ostrov needs, and what each thing brings

ostrov runs with only Hyprland, GTK 4 and gtk4-layer-shell; everything else adds a part, and its absence takes
only that part away. `ostrov doctor` says what this machine has.

## The compositor

| | Hyprland 0.53+ | another wlroots compositor |
|---|:---:|:---:|
| The bar, panels, launcher, notifications, OSD, clipboard, wallpaper | ✅ | ✅ |
| Lock screen (ext-session-lock), idle (ext-idle-notify) | ✅ | ✅ where the compositor has the protocols |
| Workspaces in the bar | ✅ | ❌ |
| The window's title | ✅ | ❌ |
| The keyboard layout block | ✅ | ❌ |
| Panels closing on a click elsewhere (Hyprland's focus grab) | ✅ | ❌ (they close as the keyboard leaves) |
| Blur under the bar and panels, its rules added by ostrov | ✅ | ❌ |
| Keys bound by ostrov | ✅ | ❌ |
| Screens off when idle | ✅ | ❌ |
| The screen-share indicator | ✅ | ❌ |
| Screen-share picker (xdg-desktop-portal-hyprland) | ✅ | ❌ |
| Plugins `night` and `displays` | ✅ | ❌ |

## The system's services

| Service | Package (Debian/Ubuntu · Arch · Fedora) | Without it |
|---|---|---|
| iwd or NetworkManager | `iwd` or `network-manager` · `iwd` or `networkmanager` · same | the Wi-Fi widget empty |
| BlueZ | `bluez` | the Bluetooth widgets empty |
| UPower | `upower` | no battery, no battery warnings |
| power-profiles-daemon | `power-profiles-daemon` | no Power Mode; the `games` plugin has nothing to switch |
| logind | systemd's | no brightness, no lock before sleep, the lock button does nothing |
| polkit | `polkitd` · `polkit` · `polkit` | no polkit agent (nothing asks) |
| PipeWire, WirePlumber | `pipewire-bin`, `wireplumber` · `pipewire`, `wireplumber` | no sound widgets, no privacy indicator |
| A Secret Service (gnome-keyring, KeePassXC) | `gnome-keyring` | secrets in `~/.local/share/ostrov/secrets.toml` (0600) |
| MPRIS players | any player | Now Playing empty |

## Programs

| Program | For | Without it |
|---|---|---|
| `wpctl`, `pw-dump`, `pw-cli` | sound | no volume, devices, apps |
| `loginctl` | the lock button | it does nothing |
| `fd` / `fdfind` | the launcher's `/` files | no file search |
| `/usr/share/unicode/emoji/emoji-test.txt` (`unicode-data` · `unicode-emoji`) | the launcher's `:` emoji | no emoji |
| `wf-recorder` | the `record` plugin | no recording |
| `udisksctl` (udisks2) | the `drives` plugin | no drives |
| `kanshi` | the `displays` plugin's profiles | its four modes alone |
| `apt`, `checkupdates` (pacman-contrib) or `dnf` | the `updates` plugin | it says it knows no package manager |
| `git` | `ostrov plugin install URL`, `ostrov theme install URL` | installs from paths alone |

## Files ostrov installs or asks to install

| File | For | Without it |
|---|---|---|
| `/etc/pam.d/ostrov` | the lock screen's own PAM profile (password only, no fingerprint wait) | it checks as `login` does |
| `/usr/lib/udev/rules.d/90-ostrov-battery.rules` (`ostrov battery limit install`) | the charge limit | the limit is root's, the toggle says so |
| `~/.config/hypr/xdph.conf`'s `custom_picker_binary` | ostrov's share picker | xdph's own picker |
