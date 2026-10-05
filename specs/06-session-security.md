# 06 — Session and security

| ID | Requirement | Status | Where / gap |
|---|---|---|---|
| LOCK-1 | ostrov SHALL lock the session itself (ext-session-lock, PAM), in its own look, without swaylock (2026-10-03) | ✅ | `src/lock.rs`, PAM profile `/etc/pam.d/ostrov` |
| LOCK-2 | Unlocking SHALL NOT wait for a fingerprint reader: password only (2026-10-02: «уберём вход по отпечатку») | ✅ | the PAM profile has no fprintd |
| IDLE-1 | ostrov SHALL lock after 10 min idle, turn the screens off after 15, lock before sleep, honour inhibitors, without swayidle (2026-10-03) | ✅ | `src/idle.rs`, `[idle]` |
| IDLE-2 | Keep Awake SHALL be a toggle (2026-10-03) | ✅ | `src/modules/system/` `awake` |
| PK-1 | ostrov SHALL be the polkit agent, its dialog in ostrov's look (2026-10-03) | ✅ | `src/polkit.rs` |
| PK-2 | IF another polkit agent runs (lxpolkit, hyprpolkitagent), THEN ostrov SHALL step aside rather than fight it, and say so; `[polkit] agent = false` turns it off (2026-10-04) | ✅ | `src/polkit.rs` `keep_out()`, the first run's welcome |
| ASK-1 | ostrov SHALL answer ssh's askpass in its look: a key's passphrase, a yes to using a key, a yes to a new host's key (2026-10-03, 2026-10-05) | ✅ | `ostrov askpass` in `src/main.rs`; dotfiles `bin/askpass` |
| ASK-2 | Its dialogs SHALL be usable from the keyboard: the focus visible, Tab moving it, Enter and Escape answering (2026-10-03) | ✅ | `src/prompt.rs` |
| ASK-3 | ssh's key passphrase SHALL be asked once a login, each use of the key confirmed (2026-10-05) | ✅ | the owner's ssh config: `AddKeysToAgent confirm` and a locked `ssh-add -c`; not ostrov's code |
| DLG-1 | The UI kit's dialogs (confirm, text, secret, choice, a settings form) SHALL be open to plugins (2026-10-04: «ui кит должны войти окна как в аскпасс ... через плагин») | ✅ | `src/prompt.rs`, `docs/plugins.md` Dialogs |
| SHARE-1 | ostrov SHALL be xdg-desktop-portal-hyprland's share picker: screens, windows, a region, in its look (2026-10-04) | ✅ | `src/share.rs`, `ostrov-share-picker` |
| SHARE-2 | WHILE the screen is shared, the privacy block SHALL show it; a click SHALL stop the share (2026-10-04) | ✅ | `src/bar/privacy.rs` |
| SHARE-3 | Browsers' own "… is sharing your screen" bars SHALL stay out of sight (2026-10-05) | ✅ | not ostrov: a Hyprland rule in the owner's dotfiles; could be a recommendation in `ostrov hyprland` |
| SEC-1 | Secrets (a plugin's token) SHALL go to the Secret Service, never to the config file (2026-10-04) | ✅ | `src/settings/secret.rs` |
| SEC-2 | The power menu SHALL suspend, restart, power off and log out (2026-10-04) | ✅ | `src/modules/system/widget.rs` |
| SEC-3 | WHEN restart, power off or log out is picked, ostrov SHALL ask first in its dialog: open apps close, unsaved work is lost (2026-10-05) | ✅ | `src/modules/system/widget.rs` `confirm()`; switching the user was not asked for |
| LOCK-3 | The lock screen SHALL be opaque whatever the theme, palette or [colors] say (2026-10-05: «лок скрин был прозрачный») | ✅ | its own black ground and a top-priority style (`lock.rs` opaque); the cause seen once not found |
| LOCK-4 | WHERE `[lock] background = "blur"`, the lock's ground SHALL be the screen as it was, blurred and darkened; black where the screen cannot be taken in a second (2026-10-05) | ✅ | `lock.rs` SHOT, `.lock-shot` |
