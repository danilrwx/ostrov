# 09 — Official plugins

Moved out of the core 2026-10-04 («caldav в плагин да, ночной свет ... в плагин, режим игр в плагин, запись экрана
в плагин, профили мониторов ... плагин под утилиты и их API»). Rust, the `ostrov-plugin` SDK, in `plugins/`.

| ID | Requirement | Status | Where / gap |
|---|---|---|---|
| PLG-1 | caldav: the events of CalDAV calendars (Yandex, iCloud, Nextcloud) in the calendar, the password in the Secret Service (2026-10-03, picked with Yandex) | ✅ | `plugins/caldav` |
| PLG-2 | night: the night light in the brightness widget's menu, by time or sunset to sunrise, its warmth set by a slider that shows the temperature as it moves (2026-10-03) | ✅ | `plugins/night` |
| PLG-3 | games: a power profile while a game is in focus, back after; its list of games the user's, empty by default (2026-10-03, 2026-10-04) | ✅ | `plugins/games` |
| PLG-4 | record: screen recording of a region or the screen, through wf-recorder (2026-10-03, picked) | ✅ | `plugins/record` |
| PLG-5 | displays: Super+P for laptop only, external only, extend, mirror, kanshi's profiles, through hyprctl and kanshi (2026-10-04) | ✅ | `plugins/displays` |
| PLG-6 | drives: removable drives mounted and ejected through udisks, no udiskie (2026-10-04) | ✅ | `plugins/drives` |
| PLG-7 | hello: the example a plugin author starts from | ✅ | `plugins/hello`, `examples/plugins/hello-python` |
| PLG-8 | updates: the packages waiting (apt, pacman's checkupdates, dnf), a badge in the bar while some wait, Update run in `$TERMINAL` (2026-10-05) | ✅ | `plugins/updates` |
| PLG-9 | google, claude launcher engines as plugins | 🚫 | replaced by `[launcher] engines` (2026-10-04) |
