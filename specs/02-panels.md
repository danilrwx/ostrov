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
| PNL-14 | The UI kit SHALL hold every element a widget needs: toggles, sliders, round buttons, rows, menus, cards, badges, dialogs; one of each (one slider, one chip flow, `chip` and `primary` buttons), each shortening its words rather than cut, colours only as tokens (2026-10-04, 2026-10-05) | ✅ | `src/ui.rs`, `src/prompt.rs` |
| PNL-15 | The System widget SHALL show the processor's load, memory in use, temperature and the network's speed (2026-10-05, from comparing with ashell) | ✅ | `src/modules/sysinfo.rs` |
| PNL-16 | A panel's width SHALL be a number of cells (4 to 12), set in Edit (Width − N +), `[panels.ID] cols` or `ostrov panel.ID cols N`; the control centre 6 by default, the calendar 8; a panel SHALL be exactly as wide as its cells, wider with more, whatever its content (2026-10-05) | ✅ | `src/cc/grid.rs` fit/rescale/base, `Panel::set_cols` |
| PNL-17 | Widgets' sizes, declared on a grid of 8, SHALL keep their share on any width (half half, the whole width the whole width); a widget SHALL choose what it shows by that share (a toggle's words) (2026-10-05) | ✅ | `grid::fit`, `grid::base` |
| PNL-18 | Changing the width back and forth SHALL NOT drift the layout: every width laid out from the layout of eight last edited, kept in panel.toml (2026-10-05) | ✅ | `Panel.base`, `keep()` |
| PNL-19 | A tile SHALL be aligned across its cells: fill, left, centre, right (2026-10-05) | ✅ | inspector's Align, panel.toml `align` |
| PNL-20 | A tile SHALL be on the panel always or only while its widget is active (Drives while a drive is plugged in), the others closing up its place; in Edit every tile shows (2026-10-05) | ✅ | inspector's On the panel, panel.toml `while_active`, `Panel::shown` |
| PNL-21 | Every widget with settings SHALL have a gear in its menu's head to them; a panel's Edit SHALL show its own settings (name, icon) and a tile's in its inspector; a width set in a form SHALL take at once (2026-10-05) | ✅ | `src/cc/mod.rs` tile, fill_gallery, the on_config hook |
| PNL-22 | Buttons for the panels SHALL open the bar's editor and restart ostrov (2026-10-05) | ✅ | `system` module's `edit-bar`, `restart` |
| PNL-23 | Every widget SHALL show a hover: its badge's by default (the battery's time left, a toggle's state, a slider's level), a plugin's its tree's `tooltip`; hovers SHALL show in an open panel too, drawn in the bar's window (GTK's own popups are held back by Hyprland's focus grab) (2026-10-05) | ✅ | `cc` draw, `popup::hover_tips`, plugins' `tooltip` |
| PNL-24 | A tile's content SHALL never be cut on any grid: a cell as wide whatever the panel's width in cells, words shortened, the inspector wrapping rather than widening the panel; sliders SHALL go half wide, two to a row; the player SHALL go two cells square, its play over the cover (2026-10-05: «в чём смысл такого обрезания?») | ✅ | `Spec::width_for`, `ui::chips`, `SLIDER` |
| PNL-25 | One slider for widgets, the Settings and plugins: its track as [appearance] sliders says (bar, thin with a round knob), a bubble with its value over the knob while it moves, a dot on each step and kept to them when it has 12 or fewer (2026-10-05) | ✅ | `ui::Slider`, `popup::bubble`, plugins' `steps` |
