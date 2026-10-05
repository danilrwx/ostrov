# Gaps

Every requirement not done (❌), half done (🟡) or undecided (❓), by priority. The ids link back to their specs.

## Bugs against what was asked

| ID | Gap | Size |
|---|---|---|
| BAR-14 | The battery's icon does not redden below 30% (red at 10%): asked 2026-10-03, lost in the move from Quickshell | small |
| PNL-11 | F10 a second time may not close Settings (seen 2026-10-04); to check | small |
| DOC-2 | README still describes `ostrov-ctl` for dwl, which is gone | small |
| P-11 | sway left in the code (`src/wm.rs`, the power menu's `swaymsg exit`) though ostrov is Hyprland-only | small |

## Wanted, not done

| ID | Gap | Size |
|---|---|---|
| A11Y-1 | Panels' widgets usable from the keyboard (focus, arrows, Enter) | medium |
| A11Y-2 | Accessible names and roles for Orca | medium |
| SEC-3 | Confirm before log out; switch user | small |
| BAR-18, EXT-10 | A plugin's own block in the bar | medium |
| PLG-8, SYS-2 | The updates plugin (apt or PackageKit) | small |
| NET-7 | NetworkManager VPN toggles | small |
| EXT-11 | A catalogue of official plugins (`ostrov plugin install NAME`) | medium |
| EXT-1 | The WebAssembly plugin backend | large |
| DOC-3 | A docs page per module and widget with its keys | medium |
| DOC-4 | Troubleshooting | small |
| DOC-5 | A requirements matrix by backend and service | small |
| DOC-6 | An architecture page for contributors | small |
| macOS 6 | A Dock (asked 2026-10-03: «6 делаем»), not started | large |
| macOS 5 | Window snapping like Rectangle (asked how hard, 2026-10-03), the compositor's more than ostrov's | ❓ |

## Not built or not run

| ID | Gap |
|---|---|
| DIST-2 | PKGBUILD, .deb and the Nix flake never built |
| DIST-3 | CI never run |

## Undecided

| ID | Question |
|---|---|
| EXT-17 | Keep `ostrov-ctl` (no user since dwl went) or remove it? |
| EXT-18 | Settings as separate small utilities: parked by the owner |
| PUB-2 | Publishing: the GitHub name and the employer's consent |
| P-8 | Further memory cuts, if wanted, need a new approach |
