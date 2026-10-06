# Themes

A theme is a directory: a `theme.toml` with its colours and a few suggestions, and, for changes past colours, an
optional `theme.css`. ostrov's five (Dark, Light, Graphite, Nord, Solarized) are the same files, under `themes/` in
the source and compiled in; yours live in `~/.local/share/ostrov/themes/<id>/`.

    ostrov theme list                  the themes, built in and installed, the one picked starred
    ostrov theme set ID                picks one ([appearance] theme = "ID")
    ostrov theme install PATH|GIT-URL  installs one from a directory or a git repository
    ostrov theme remove ID             removes an installed one

The Appearance page in the control centre (the palette beside Edit, `ostrov appearance`) shows every theme as a card
in its own colours; a click picks it. Picking, installing and removing take effect at once.

## The id

The directory's name: of `[a-z0-9-]`. Installed from a git URL, it is the repository's name less `.git` and an
`ostrov-theme-` in front (`https://example.com/me/ostrov-theme-paper.git` is `paper`). Installing a theme of an id
already installed replaces it (an upgrade). An installed theme with a built-in one's id stands in for it; removing it
brings the built-in one back. A theme that does not read is refused at install, and shown with why in
`ostrov theme list`.

## theme.toml

```toml
name = "Paper"            # on its card (required)
author = "me"
version = "1.2"
dark = false              # whether it is dark (required): GTK's own widgets, the colour dialog, follow it

# suggestions, taken while [appearance] says nothing of them
radius = 6                # a surface's corners in px, 0 to 20; what is on it 4 less
density = "compact"       # compact, normal, comfortable: the control centre's rows
blur = true               # Hyprland's blur behind the bar and what opens

[colors]                  # any of the tokens below; the rest stay ostrov's (Dark's)
surface = "#fafafa"
fg = "#202020"
accent = "#3a6ea5"
```

A colour is anything GTK's CSS takes: `#rrggbb`, `rgb()`, `rgba()`, a name, another token (`@fg`), or a function of
one (`shade(@accent, 0.8)`, `alpha(@fg, 0.1)`, `mix(@fg, @surface, 0.5)`).

### What wins

Later over earlier:

1. ostrov's palette (style.rs; what Dark is);
2. the theme's `[colors]`;
3. `[appearance]`'s `accent` and `surface` in config.toml (the accent also sets `ink`, `accent-pressed` and
   `accent-rule` to go with it), and its `opacity` over the surface's colour;
4. config.toml's `[colors]`.

So a theme sets the look, the Appearance page's accent and surface adjust it, and `[colors]` overrides any token.
The same for the suggestions: `[appearance]`'s `radius`, `density` and `blur`, when given, over the theme's.

## The tokens

| token | colours |
|-------|---------|
| `fg` | text and icons |
| `dim` | what is said second: a subtitle, a time, a help line, a day of another month |
| `ink` | text and icons on the accent |
| `accent` | on, picked, today, a slider's level, a focused workspace |
| `accent-rule` | a line on the accent (a toggle's divider while on) |
| `accent-pressed` | the accent pressed (a toggle's open side) |
| `surface` | the ground of everything that opens: panels, the calendar, menus, toasts, the OSD. Its colour only: the opacity is `[appearance] opacity` |
| `bar` | the bar's ground; `ALPHA` in it is replaced by the bar's see-through ([widget.wallpaper] bar), as in `rgba(0, 0, 0, ALPHA)` |
| `hover` | under the pointer |
| `raised` | a button, a toggle, a chip on a surface |
| `card` | a card, a menu, the month on a surface |
| `well` | a slider's trough, a switch off, a scrollbar |
| `sunk` | an entry |
| `rule` | an outline, a separator; a toggle's subtitle while on |
| `idle` | a workspace not focused |
| `handle` | a tile's corner in Edit |
| `urgent` | an error, a critical notification |
| `recording` | the mic or the camera in use |
| `lock` | the lock screen and the login screen |
| `ground` | under the wallpaper, and in its place with none |
| `shade` | the screen dimmed under a question (polkit's, askpass's) |
| `scrim` | under a button over a picture (play on the player's cover) |
| `on-scrim` | an icon on the scrim and on a tile's corner in Edit |

A light theme sets `fg`, `dim`, `ink`, `accent`, `hover`, `raised`, `card`, `well`, `sunk`, `rule` and `bar` at least:
ostrov's own are white on black. Mind that the surface is see-through: over a dark wallpaper a light surface at
0.75 reads grey, so `dim` wants to be darker than on paper (Light's is `#505055`).

## theme.css

Laid after all of ostrov's CSS, so its rules win over ostrov's of the same weight. It is GTK 4's CSS; the tokens are
there as `@name`, `ALPHA` as above. Keep to the classes below: they are the stable API, kept and meaning the same
across releases. Anything else (a widget's inner nodes, an order of children) may change.

| class | what |
|-------|------|
| `.surface` | the ground of anything that opens; `.surface.attached` one grown out of the bar |
| `.toast`, `.osd` | a notification's toast, the OSD (on a `.surface`); `.toast.critical` |
| `.card`, `.menu` | a card on a surface, a widget's unrolled menu; `.card.critical` |
| `.cc` | the control centre's (and any panel's) grid; `.cc.editing` in Edit |
| `.tile` | a widget's place in the grid; `.tile.open` its menu open |
| `.toggle`, `.toggle.on` | a toggle tile; its parts `.toggle-main`, `.toggle-side`, `.toggle-title`, `.toggle-sub` |
| `button.round` | a round button tile (lock, screenshot); `.open` while its menu is |
| `button.arrow` | a slider's or toggle's arrow to its menu |
| `button.item`, `button.item.on` | a row in a menu (a network, a device), the one in use |
| `button.chip`, `button.chip.picked` | a small choice button |
| `button.connect` | a menu's main action |
| `.battery`, `.clock`, `.month` | the battery's card, the clock, the month (a GTK `calendar` inside) |
| `.plugin-card` | a plugin's or a KDL widget's card |
| `.badge` | a widget's badge |
| `.title`, `.bold`, `.dim`, `.error`, `.date` | words |
| `.bar-bg`, `.slot`, `.pill` | the bar's ground, a block's slot, a block (`.slot.tab` the one whose popup is open) |
| `.dot`, `.dot.focused` | a workspace's dot |
| `.tray-menu` | a tray item's menu |
| `.hit`, `.hit.picked` | a launcher's hit, the one picked |
| `.preview` | the clipboard entry picked, under the launcher |
| `window.lock`, `.lock-time`, `.lock-entry` | the lock screen |
| `window.prompt` | a question (polkit's, askpass's) |
| `.page-title`, `.field`, `.field-help` | the control centre's Settings and Appearance pages |

GTK's own nodes (`scale`, `switch`, `entry`, `calendar`, `scrollbar`, `separator`) can be styled too, as GTK names
them.

Corner radii in theme.css are taken as written; `[appearance] radius` and the theme's `radius` change ostrov's own.

## An example

`examples/themes/catppuccin-mocha/`: Catppuccin's Mocha palette as tokens, a larger radius, and a `theme.css` that
draws the focused workspace as a pill and rings the toggles that are on.

    ostrov theme install examples/themes/catppuccin-mocha
    ostrov theme set catppuccin-mocha

To publish a theme, put the directory's files at a git repository's top, named `ostrov-theme-<id>`;
`ostrov theme install https://.../ostrov-theme-<id>.git` then installs it as `<id>`.

## The other apps

As the appearance changes, ostrov puts its look into the rest of the desktop (`[appearance] apps = false` stops it):

- **The scheme and the accent**, in GNOME's settings (`org.gnome.desktop.interface color-scheme` and
  `accent-color`, the named accent nearest ostrov's). The settings portal (xdg-desktop-portal-gtk) hands them on:
  GTK and libadwaita apps, Qt 6.5+ ones (Telegram), Chrome and Firefox go dark or light with it.
- **GTK's palette**: `~/.config/gtk-3.0/ostrov.css` and `~/.config/gtk-4.0/ostrov.css`, libadwaita's and
  adw-gtk3's named colours (the accent, the window's, the views', the sidebars', the cards') and GTK 4.16's
  variables; `gtk.css` beside each imports it (a line put at its top). File managers and GNOME's apps take it as
  they start.
- **Colour files** in `~/.local/state/ostrov/colors/`, for what a config can import:

| File | Its app |
|---|---|
| `alacritty.toml` | `import = ["~/.local/state/ostrov/colors/alacritty.toml"]`; alacritty reads it again as it changes |
| `kitty.conf` | `include ~/.local/state/ostrov/colors/kitty.conf`; kitty is told to read it again |
| `foot.ini` | `[main] include=~/.local/state/ostrov/colors/foot.ini`, at foot's start |
| `telegram.tdesktop-palette` | opened in Telegram Desktop once, as a theme of one's own |
| `colors.sh`, `colors.json` | scripts |

- **Templates of your own**: a file in `~/.config/ostrov/templates/` is written to `colors/` under its name with
  `{{bg}}`, `{{view}}`, `{{card}}`, `{{sidebar}}`, `{{fg}}`, `{{dim}}`, `{{accent}}`, `{{ink}}`, `{{urgent}}` and
  `{{ansi0}}` to `{{ansi15}}` filled in (One Dark's or One Light's colours, as the theme is dark or light).
