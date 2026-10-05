# 05 — Notifications, OSD, warnings

| ID | Requirement | Status | Where / gap |
|---|---|---|---|
| NOTE-1 | ostrov SHALL be the notification server (org.freedesktop.Notifications), showing toasts under the bar and a history in a widget (2026-10-03) | ✅ | `src/notes.rs` |
| NOTE-2 | Notifications' action buttons SHALL work (2026-10-03) | ✅ | `src/notes.rs` |
| NOTE-3 | WHEN a notification offers inline reply (Telegram and other chats), ostrov SHALL let the user type the answer in it; its send button SHALL look like ostrov's buttons (2026-10-04) | ✅ | `src/notes.rs` `Reply` |
| NOTE-4 | Do Not Disturb SHALL be a toggle; quiet SHALL also come from a schedule, from a game in focus, with apps allowed through (2026-10-04) | ✅ | `[notifications] quiet_from/quiet_to/quiet_in_games/allow` |
| NOTE-5 | IF another notification daemon (mako, dunst) owns the name, THEN ostrov SHALL say so and not fight it | ✅ | `ostrov doctor` |
| OSD-1 | Volume, brightness and the mic's mute keys SHALL show an OSD in ostrov's style, with icons and rounded more (2026-10-03) | ✅ | `src/keys.rs` |
| OSD-2 | ostrov SHALL handle the media and brightness keys itself, without wpctl, brightnessctl or a script; brightness in 5% steps, volume in 2% (2026-10-01, 2026-10-02, 2026-10-03) | ✅ | `src/keys.rs` |
| OSD-3 | The laptop's vendor keys (settings, notification centre) SHALL open ostrov's panels | ✅ | `XF86Tools`, `XF86NotificationCenter` in KEYS |
| WARN-1 | WHILE discharging, ostrov SHALL warn at 20% and 10%, and at 5% ask to suspend (2026-10-03, picked from the list) | ✅ | `src/keys.rs` `battery()` |
