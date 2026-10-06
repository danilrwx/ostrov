# 10 — Distribution, first run, docs

| ID | Requirement | Status | Where / gap |
|---|---|---|---|
| DIST-1 | ostrov SHALL install with `cargo install`; its dependencies SHALL be listed for Debian/Ubuntu, Arch, Fedora (2026-10-04) | ✅ | README Requirements |
| DIST-2 | Packages SHALL exist: a PKGBUILD, a .deb through cargo-deb, a Nix flake, the PAM profile (2026-10-04) | 🟡 | the .deb built and installed in a clean Ubuntu 26.04 container; the PKGBUILD fixed, not built yet; the flake never built |
| DIST-3 | CI SHALL build, test and lint (clippy, warnings fatal) the workspace (2026-10-04, 2026-10-05) | ✅ | `.github/workflows/ci.yml`; its steps run locally in an Arch container |
| RUN0-1 | WHEN started the first time, ostrov SHALL welcome the user: theme, accent, density, language, and honestly say what it sets up in the system (2026-10-04) | ✅ | `src/welcome.rs` |
| RUN0-2 | `ostrov doctor` SHALL check what ostrov needs and say how to fix each thing (2026-10-04) | ✅ | `src/doctor.rs` |
| RUN0-3 | `ostrov hyprland` SHALL print the hyprland.conf lines it recommends (keys, rules, gestures) without writing them (2026-10-04) | ✅ | `src/modules/hyprland.rs` |
| RUN0-4 | Three-finger workspace swipes SHALL be recommended (macOS list item 1, 2026-10-03) | ✅ | recommended by `ostrov hyprland`, not added |
| PUB-1 | The history SHALL carry the owner's personal address, nothing of the employer or Yandex (2026-10-04) | ✅ | 176 commits by the GitHub noreply address |
| PUB-2 | ostrov SHALL be public (2026-10-05) | ✅ | github.com/danilrwx/ostrov, the full history, branch main |
| DOC-1 | The README SHALL sell ostrov to its user: short, what it gives, screenshots made in a sandbox (nothing personal, dark theme, the control centre 8 cells wide), install in three steps, the details in docs/install.md (2026-10-04, 2026-10-05, 2026-10-06) | ✅ | `README.md`, `docs/screenshots/make.sh` (a nested Hyprland's headless output alone) |
| DOC-2 | The README SHALL not describe what is gone: the dwl status line of `ostrov-ctl` (dwl dropped 2026-10-05) | ✅ | removed |
| DOC-3 | Docs SHALL have a page per module and widget with its config keys (ashell has one per module, 2026-10-05) | ✅ | `docs/modules.md` |
| DOC-4 | Docs SHALL have troubleshooting: what `ostrov doctor` reports and how to fix it (2026-10-05) | ✅ | `docs/troubleshooting.md` |
| DOC-5 | Docs SHALL have a requirements matrix: what works without NetworkManager, BlueZ, power-profiles-daemon, UPower, and what needs which Hyprland version (2026-10-05) | ✅ | `docs/requirements.md` |
| DOC-6 | Docs SHALL have a contributor's architecture: the hub, services, modules, the popup host, the plugin host (2026-10-05) | ✅ | `docs/architecture.md` |
| A11Y-1 | Panels and the launcher SHALL be fully usable from the keyboard (2026-10-04's GNOME comparison) | 🟡 | the first widget focused on opening, Tab and arrows through GTK, a slider ringed; to check live after Super+X (an exclusive keyboard closed the popup and was undone) |
| A11Y-2 | ostrov SHALL work with Orca | 🚫 | not wanted (2026-10-05) |
| I18N-1 | Adding a language SHALL be a directory of catalogues, a test checking them (2026-10-04) | ✅ | `i18n/`, `src/i18n.rs` |
