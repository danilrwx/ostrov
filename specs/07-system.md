# 07 — The system's modules

| ID | Requirement | Status | Where / gap |
|---|---|---|---|
| SND-1 | Volume and mic SHALL be sliders with a menu of outputs and inputs, sorted so they never reorder (2026-10-03) | ✅ | `src/modules/audio/` |
| SND-2 | WHEN an output is picked, the sound SHALL go there (the headphones, not the speakers), as reliably as the old shell script did (2026-10-01, 2026-10-04) | ✅ | `src/modules/audio/service.rs` |
| SND-3 | A card's ports and other profiles' outputs SHALL be listed too, a click switching the profile (2026-10-04) | ✅ | `src/modules/audio/service.rs` |
| SND-4 | Each app playing SHALL have its own slider (2026-10-03, picked) | ✅ | `src/modules/audio/widget.rs` |
| SND-5 | The headset button SHALL show only while a Bluetooth headset is connected, at the right of "Output" (2026-10-03) | ✅ | `audio::HEADSET` |
| NET-1 | Wi-Fi SHALL scan, join, forget, through iwd or NetworkManager (2026-10-03: «проверь совместимость iwd») | ✅ | `src/modules/wifi/` |
| NET-2 | WHEN a network needs a passphrase, its field SHALL open under that network, with a confirm button, aligned (2026-10-03) | ✅ | `src/modules/wifi/widget.rs` |
| NET-3 | WHILE joining, the row SHALL switch to the joined state by itself (2026-10-03) | ✅ | |
| NET-4 | The Wi-Fi's name and Bluetooth need not be in the bar all the time (2026-10-05) | ✅ | their badges `Show::Active` or never by default |
| NET-5 | Airplane Mode SHALL turn every radio off (2026-10-04, GNOME comparison) | ✅ | `src/modules/airplane/`, `src/services/rfkill.rs` |
| NET-6 | VPNs (the owner's VLESS and OpenVPN) SHALL be toggles with their status, named by what they are ("VLESS", not "VPN") (2026-10-01, 2026-10-02) | ✅ | KDL widgets in the owner's dotfiles (`examples/widgets/vpn.kdl` the pattern); not in ostrov's core |
| NET-7 | NetworkManager's VPN connections SHALL be toggles (GNOME comparison) | ❌ | gap: offered as a plugin or widget over `nmcli`, not done |
| BT-1 | Bluetooth SHALL power, pair, connect, show connected devices' batteries (2026-10-03) | ✅ | `src/modules/bt/` |
| PWR-1 | Power profiles SHALL be switchable (power-profiles-daemon) (2026-10-03) | ✅ | `src/modules/power/` |
| BAT-1 | The battery widget SHALL show the charge, its state and the time left (2026-10-03) | ✅ | `src/modules/battery/` |
| BAT-2 | The charge limit SHALL be a toggle (80% or full) with a menu of limits, needing a udev rule installed once (2026-10-04) | ✅ | `src/modules/battery/limit.rs` |
| BRI-1 | Brightness SHALL be a slider through logind, without brightnessctl | ✅ | `src/modules/brightness/` |
| BRI-2 | The night light SHALL be inside the brightness slider's menu (its switch, modes, warmth), as it was, not a tile of its own (2026-10-05: «иначе выглядит стрёмно») | ✅ | a plugin widget's `attach = "brightness"`, `plugins::attached` |
| KBD-1 | The keyboard layout SHALL be shown and switched (2026-10-03) | ✅ | `src/modules/keymap/` |
| MED-1 | The player (MPRIS) SHALL be a widget with its controls, the media keys acting on it (2026-10-03) | ✅ | `src/modules/media/` |
| WTH-1 | The weather SHALL be in the core, by a location set by city or coordinates (2026-10-04: «погода пусть будет в ядре») | ✅ | `src/modules/weather/`, `ostrov location` |
| CAL-1 | The calendar SHALL show the month and what is coming up, its events from plugins and .ics (2026-10-03) | ✅ | `src/modules/calendar/`; CalDAV is a plugin (09) |
| WALL-1 | ostrov SHALL draw the wallpaper itself, without swaybg; plain black by default (2026-09-29, 2026-10-03) | ✅ | `src/wallpaper.rs` |
| SYS-1 | The processor's load, memory, temperature and the network's speed SHALL be a widget (2026-10-05) | ✅ | `src/modules/sysinfo.rs` |
| SYS-2 | Pending system updates SHALL be shown (ashell has it, 2026-10-05; GNOME comparison) | ✅ | the official plugin `updates` (09) |
| SYS-3 | ostrov SHALL work without NetworkManager, through iwd, and say what is missing without either | ✅ | iwd supported; `docs/requirements.md` says what each service brings |
