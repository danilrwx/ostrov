# Apps in ostrov's colours

As its look changes, ostrov colours the rest of the desktop. The toolkits it does itself (`[appearance] apps`,
docs/themes.md, "The other apps"): GNOME's scheme and accent for the portal, GTK 3's theme of its own, GTK 4's
palette, Chrome's frame through its GTK theme. Everything else is an **integration**: an app it writes a colour
file for, and whose config it can make read that file.

    ostrov apps                      every integration: here or not, connected or not, built in or yours, its file
    ostrov apps connect ID           its config made to read ostrov's file (the line put in, once)
    ostrov apps disconnect ID        the line taken out again
    ostrov apps apply                every file written again now
    ostrov apps catalogue            the integrations to install, the installed marked
    ostrov apps install ID|PATH|GIT-URL   one from the catalogue by its id, a directory, or a git URL (#path in it)
    ostrov apps remove ID            one of yours taken away (disconnected first)

The Appearance page's **Apps** section has the same as switches, for the apps on this machine. A file is written
for every app there is, connected or not, in `~/.local/state/ostrov/colors/`; connecting is only about the app's
config. The ones connected are kept in `[apps] connected`.

Built in:

| App | Connected, its config | Then |
|---|---|---|
| alacritty, kitty, foot, ghostty | its colour file included | at once (foot's open windows told the colours by escape sequences) |
| WezTerm | `colors/ostrov.toml` linked | `config.color_scheme = "ostrov"` |
| tmux | `source-file` in tmux.conf | the server reads it at once |
| Neovim | `colors/ostrov.lua` linked | `:colorscheme ostrov`; every nvim on it loads it again at once |
| Helix | `themes/ostrov.toml` linked | `theme = "ostrov"`; told to read it at once |
| btop | `themes/ostrov.theme` linked | picked in its options |
| k9s | `skins/ostrov.yaml` linked | `ui.skin: ostrov` |
| fzf | sourced from .zshrc (or .bashrc) | as a shell starts |
| zathura | `include` in zathurarc | as it starts |
| Discord (Vesktop) | Vencord's `themes/ostrov.theme.css` linked | on in its Themes; at once |
| Hyprland's borders | `source` in hyprland.conf | the running one's set at once |
| Telegram | none: its palette to open once as a theme | |
| Shell scripts | none: `colors.sh`, `colors.json` | |

## An integration of your own

A directory in `~/.config/ostrov/apps/<id>/` (one of a built-in's id stands in for it): an `app.toml` and its
template beside it.

```toml
name = "btop"
detect = ["btop"]                  # commands one of which in PATH, or paths (~/...) one of which there
template = "ostrov.theme"          # the file beside, written to ~/.local/state/ostrov/colors/ostrov.theme

[include]                          # how its config reads that file (left out: a file to open by hand)
file = "~/.config/btop/btop.conf"
line = 'color_theme = "~/.local/state/ostrov/colors/ostrov.theme"'
# section = "[main]"               # the line right under this section's header (foot's [main])
# key = "general.import"           # instead: a TOML array of paths the file goes first in (alacritty's)
# link = "~/.config/k9s/skins/ostrov.yaml"   # instead: a link to the file

[reload]                           # how it reads the file again as it changes (left out: at its start)
signal = "USR1"                    # a signal to the processes of that name
process = "btop"
# command = "..."                  # or a command (the palette's {{tokens}} filled in)
# osc = true                       # or the colours sent into the terminals of the processes named (foot's way)
```

The template is the app's own format with ostrov's tokens in it, filled in as the look changes:

| Token | |
|---|---|
| `{{bg}}` | a window's ground: the bar's colour with no wallpaper, the panels' surface over one |
| `{{bar}}`, `{{bar_alpha}}` | the bar's colour and how solid it is (0 to 1): a terminal's ground and see-through |
| `{{view}}`, `{{card}}`, `{{sidebar}}` | grounds a little lighter (darker by day) than `{{bg}}` |
| `{{fg}}`, `{{dim}}` | text, and what is said second |
| `{{accent}}`, `{{ink}}` | the accent, and what goes on it |
| `{{urgent}}` | an error |
| `{{mode}}` | `dark` or `light` |
| `{{ansi0}}` … `{{ansi15}}` | a terminal's sixteen: One Dark's after dark, One Light's by day |

`{{name|bare}}` is a colour without its `#` (foot's way). Connecting puts the line in once (a config that is a link,
as dotfiles keep them, is written through it); disconnecting takes only that line out, or only the file's entry in
an array.

## From a plugin

A plugin brings integrations in its `manifest.toml`, each an `[[apps]]` with an `id` and the same keys as an
`app.toml`, its template a file in the plugin's directory. Its id becomes `<plugin>-<id>`; it is there while the
plugin is on.

```toml
[[apps]]
id = "cava"
name = "cava"
detect = ["cava"]
template = "cava-colors"           # plugins/<plugin>/cava-colors

[apps.include]
file = "~/.config/cava/config"
line = "..."
```

For what a file cannot do (an app with an API of its own, a browser's extension), a plugin follows the `palette`
event (`events = ["palette"]`, the permission `events`): every token, as `colors.json` has them, whenever the look
changes, in `on_shell_event("palette", {...})`.

## The catalogue

Integrations past the built-in ones, installable by their id: `ostrov apps install rofi`, or Install on the
Appearance page's Apps. They live in ostrov's repository under `catalogue/apps/<id>/` with an `[[app]]` entry in
`catalogue.toml`; one of yours goes there by a pull request. Installed, an integration is a directory of yours in
`~/.config/ostrov/apps/<id>/` like any other. Now: rofi, qutebrowser.
