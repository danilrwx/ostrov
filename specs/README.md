# ostrov's specs

What ostrov must do, written before the code is, and how far the code is from it. Spec-driven: a change starts
here (a requirement added or changed), then the design if it needs one, then the code; a requirement's status is
kept true as the code moves.

## The files

| File | What |
|---|---|
| [00-product.md](00-product.md) | what ostrov is, for whom, the principles every other spec answers to |
| [01-bar.md](01-bar.md) | the bar: zones, blocks, monitors, hiding |
| [02-panels.md](02-panels.md) | panels of widgets: the grid, sizes, the gallery, Edit, the faces in the bar |
| [03-look.md](03-look.md) | themes, colours, transparency, blur, density, animations |
| [04-launcher-clipboard.md](04-launcher-clipboard.md) | the launcher in the bar, its modes, the clipboard's history, screenshots |
| [05-notifications-osd.md](05-notifications-osd.md) | the notification server, toasts, quiet, the OSD, battery warnings |
| [06-session-security.md](06-session-security.md) | lock, idle, polkit, askpass, the share picker, privacy |
| [07-system.md](07-system.md) | the system's modules: sound, Wi-Fi, Bluetooth, power, battery, brightness, media, weather... |
| [08-extensibility.md](08-extensibility.md) | plugins, KDL widgets, settings' API, events, keys, D-Bus, commands |
| [09-plugins.md](09-plugins.md) | the official plugins |
| [10-distribution-docs.md](10-distribution-docs.md) | install, packages, doctor, the first run, docs, publishing |
| [decisions.md](decisions.md) | what was tried and dropped, and why: not to be proposed again without a new reason |
| [gaps.md](gaps.md) | every open gap in one list, by priority |

## A requirement

Each is a row: an id, the requirement in EARS form, its status, where the code is or what is missing, and where it
came from (the date the owner asked for it).

EARS, the five shapes:

- ubiquitous: *ostrov SHALL ...*
- event: *WHEN \<trigger\>, ostrov SHALL ...*
- state: *WHILE \<state\>, ostrov SHALL ...*
- optional: *WHERE \<feature is present\>, ostrov SHALL ...*
- unwanted: *IF \<condition\>, THEN ostrov SHALL ...*

Status:

| Mark | Meaning |
|---|---|
| ✅ | done: the code does it, where it says |
| 🟡 | partial: part of it is done, the gap says what is not |
| ❌ | gap: asked for, not done |
| ❓ | open: asked about, not decided |
| 🚫 | dropped: decided against (see decisions.md) |

## Working with them

1. A new wish: add its row (❌) to the spec it belongs to, and to gaps.md.
2. Bigger than a few lines of code: a `## Design` section under the requirement (what changes, where), agreed
   before writing code.
3. Done: the row ✅ with where the code is; gaps.md loses it.
4. Dropped: the row 🚫, a line in decisions.md with the date and the reason.
