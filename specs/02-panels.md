# 02 — Panels of widgets

The owner's words (2026-10-04): «модульность, как в макоси, чтобы я могу настраивать как хочу каждый элемент и
добавлять нужные из галереи. Каждый виджет должен уметь работать в разных размерах ... сетка допустим 8 в ширину, ui
кит для возможных элементов, меню другого уровня с настройкой внешнего вида».

| ID | Requirement | Status | Where / gap |
|---|---|---|---|
| PNL-1 | A panel SHALL be a grid 8 cells wide of widgets placed and sized by the user (2026-10-04) | ✅ | `src/cc/grid.rs`, `panel.toml` |
| PNL-2 | Each widget SHALL declare the sizes it allows, its first the default (brightness the whole width only, the wallpaper's button from one cell up) (2026-10-04) | ✅ | `WidgetDef.sizes`, `src/modules/mod.rs` |
| PNL-3 | WHEN Edit is on, the user SHALL move, resize and remove widgets in place, and add them from a gallery; the resize and remove handles SHALL sit above everything, the resize one blue (2026-10-04) | ✅ | `src/cc/mod.rs` |
| PNL-4 | Edit SHALL show, past a divider, the widgets enabled but not placed (2026-10-04) | ✅ | `src/cc/mod.rs` |
| PNL-5 | The user SHALL make panels of their own and put them in the bar (2026-10-04) | ✅ | `[[panel]]`, `src/cc/mod.rs` |
| PNL-6 | The grid's density SHALL have three levels (comfortable, compact, dense), not more, so widgets still fit (2026-10-04: «лучше 3 уровня») | ✅ | `[appearance] density` |
| PNL-7 | Every widget SHALL fit its cells at every size it allows: no text cut, no overflow, blocks aligned (2026-10-04) | ✅ | each widget's `size` callback |
| PNL-8 | Sliders SHALL step in whole numbers (2026-10-04) | ✅ | `src/ui.rs` |
| PNL-9 | A list that unfolds (outputs, inputs, networks) SHALL unfold under an arrow, hidden by default, without the panel jumping (2026-10-03) | ✅ | `src/ui.rs` menus |
| PNL-10 | The control centre and the calendar SHALL be panels like any other; the calendar's month, the player, the agenda, notifications, the weather, the clock are widgets (2026-10-04) | ✅ | `src/modules/*/widget.rs` |
| PNL-11 | WHEN F10 is pressed again with Settings open, Settings SHALL close (2026-10-04: «повторное нажатие их не скрывает») | ✅ | confirmed by the owner (2026-10-05) |
| PNL-12 | A widget's settings SHALL be reachable from its place in Edit, not only from Settings (2026-10-04: «не понимаю как ... перейти в настройки этих виджетов») | ✅ | the inspector in Edit |
| PNL-13 | Panels SHALL be declared in a markup language pleasant to write by hand (2026-10-04: «красивый язык разметки для панелей») | ✅ | KDL for widgets (`src/widgets/`); the grid itself in `panel.toml` written by Edit |
| PNL-14 | The UI kit SHALL hold every element a widget needs: toggles, sliders, round buttons, rows, menus, cards, badges, dialogs (2026-10-04) | ✅ | `src/ui.rs`, `src/prompt.rs` |
| PNL-15 | The System widget SHALL show the processor's load, memory in use, temperature and the network's speed (2026-10-05, from comparing with ashell) | ✅ | `src/modules/sysinfo.rs` |
