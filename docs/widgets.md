# Widgets in KDL

A widget of your own, without code: a file in `~/.config/ostrov/widgets/*.kdl` declares one or more widgets that
join the panels' gallery like the built-in ones. **Edit** in a panel, pick it from the gallery, give it a size.
It is drawn with the same nodes as a plugin's (docs/plugins.md, Nodes), by the same renderer.

```kdl
widget "vless-cli" name="VLESS" icon="network-vpn-symbolic" sizes="4x1 2x1 1x1" {
    poll "st" every="5s" exec="vless status --json"
    toggle icon="network-vpn-symbolic" title="VLESS" sub="{st.profile}" on="{st.on}" {
        click exec="vless toggle"
        menu {
            for "p" in="{st.profiles}" {
                row text="{p}" on="{p == st.profile}" { click exec="vless profile {p}" }
            }
        }
    }
    badge icon="network-vpn-symbolic" text="{st.profile}" show="active" active="{st.on}"
}
```

The files are KDL 2 (KDL 1 is read too). Examples: `examples/widgets/` (a VPN toggle, CPU load from a poll, a
player followed by `listen`).

## Reading and editing

ostrov reads the files as it starts. Saving a file draws its widgets anew at once, their sources started again if
they changed; a widget new to the file (a new id) appears after `ostrov restart`. A file that does not read is said
on ostrov's stderr as `FILE:LINE: what`, and once in a toast; its widgets stay as they were.

## widget

    widget "ID" name="Name" icon="icon-name" sizes="4x1 2x1" { ... }

| property | |
|----------|-|
| ID (the argument) | of `[a-z0-9-]`; the widget's id in `panel.toml`, `ostrov menu ID`. An id already taken (one of ostrov's own, `vless`, `battery`..., a plugin's, another file's) is refused, said on stderr |
| `name` | in the gallery, and its menu's title; the id if left out |
| `icon` | an icon name of the theme |
| `sizes` | cells `WxH` the grid (8 wide) allows, the first its default; `4x1 2x1 1x1 8x1` if left out |

Its children: sources (`poll`, `listen`), at most one `badge`, `setting`s, and the nodes it draws: one node is its
root, more are drawn in a column.

## Sources

    poll "NAME" every="5s" exec="COMMAND"
    listen "NAME" exec="COMMAND"
    event "NAME" on="EVENT"

- `event "NAME" on="EVENT"` holds the payload of ostrov's last such event (`window` `{class, title}`,
  `workspace`, `screencast` `{on, window}`, `lock`, `unlock`, `wallpaper` `{on, path}`, `network` `{on, ssid}`,
  `power` `{state, plugged, percent}`, `bluetooth` `{on, connected}`, `output` `{name}`, `media` `{playing, title,
  artist}`), null till one comes: `event "win" on="window"` then `label text="{win.title}"`.
- `poll` runs its command through `sh -c` every so long (`30`, `"5s"`, `"10m"`, `"1h"`), and right after any of
  the widget's events has run. One taking over a minute is ended.
- `listen` runs its command and leaves it running: every line it prints is a new value. When it ends it is started
  again, 1 s later, twice as long each time it ends within a minute, up to a minute.
- What a command prints is JSON if it parses as JSON, else its text trimmed. `NAME` holds it.

They run on ostrov's main loop through GIO, never blocking it, from the moment the widget is first on a panel. Their
stderr goes to ostrov's.

Also always there: `state`, ostrov's state (`ostrov dump`), e.g. `{state.battery.percent}`; and `config`, the
widget's settings (see Settings).

## Expressions

Any string may hold expressions in braces:

| | |
|-|-|
| `{st.profile}`, `{st.profiles.0}` | a path into a value; a key or a list's index. A path to nothing is null (shown as nothing) |
| `'text'`, `"text"`, `42`, `0.5`, `true`, `false`, `null` | literals (single quotes are handier inside a KDL string) |
| `==`, `!=` | equal or not; a number and a string compare as text (`3 == '3'`) |
| `!`, `&&`, `\|\|` | not, and, or; `a \|\| b` is a if a is true, else b (`{st.name \|\| 'none'}`), `c && a \|\| b` a choice |
| `( )` | grouping |

False are `null`, `false`, `0`, `""` and `[]`; the rest are true. `{{` and `}}` are braces themselves.

A property that is exactly one `{expr}` keeps its value's type: `on="{st.on}"` is a boolean, `value="{st.level}"`
a number, `in="{st.list}"` a list. Anything else is text with the values put in: `sub="{st.n} devices"`. Then each
property is made what its node takes: `on` a switch (a chip's `on` an index), `value` a number, `options` a list,
the rest text.

## Structure

    for "x" in="{list}" { ... }     the nodes inside once for each item, as x (an object: each value)
    if "{cond}" { ... }             the nodes inside while cond is true

Both may be anywhere nodes are, and nest.

## Nodes

Those of plugins (docs/plugins.md), by the same names and properties:

| node | properties | |
|------|------------|-|
| `toggle` | `icon`, `title`, `sub`, `on` | a `menu { ... }` child: what its arrow unfolds |
| `slider` | `icon`, `value` (0 to 1) | `change` |
| `button` | `icon`, `label` | `click` |
| `round` | `icon`, `label` (its tooltip) | `click` |
| `label` | `text`, `class` (`dim`, `bold`, `title`, `error`) | |
| `row` | `icon`, `text`, `note`, `on` (ticked) | `click` |
| `box` | `orientation` (`vertical`, `horizontal`) | its children |
| `image` | `icon`, or `path` | |
| `progress` | `value` (0 to 1) | |
| `chips` | `options` (a list, or `"a b c"`), `on` (an index) | `change` |
| `entry` | `placeholder` | `change` (on Enter) |
| `separator` | | |

A root other than a toggle, a slider or a round button sits on a card. A toggle narrower than four cells is its
icon alone.

## Events

A node's children `click`, `toggle`, `change`, each with an `exec`, say what its use runs:

    row text="{p}" { click exec="vless profile {p}" }
    slider value="{st.level}" { change exec="brightnessctl set {value}" }
    entry placeholder="Search" { change exec="xdg-open https://duckduckgo.com/?q={value}" }

- `{value}` is the event's value: a toggle's new state (`true`/`false`), a slider's level (`0.42`), a chip's
  index, an entry's text. The `for`'s variables around the node are there too.
- **Every value put into an exec is quoted for sh** (in single quotes, its own escaped): `{p}` is one word whatever
  it holds, `it's; rm -rf ~` included. So never quote them yourself: `exec="cmd '{p}'"` would quote twice.
- `click` fires on a toggle's click too. A `toggle` with an `exec` is the event; one without is a toggle drawn.
- The command runs through `sh -c`, detached; once it ends (or 2 s on, if it goes on) the widget's polls are run
  again, so what it changed shows at once.

The same quoting applies to `poll` and `listen` commands (`{config.file}`).

## Badge

    badge icon="network-vpn-symbolic" text="{st.profile}" show="active" active="{st.on}"

What the widget puts into its panel's face in the bar: an icon, a few words. `show` is when, unless the panel says
otherwise (the tile's eye in Edit): `always` (the default), `active` (while `active` is true; true if left out),
`never`.

## Settings

    setting "file" title="Read from" type="path" default="/proc/loadavg"

Each a field of the widget's own page in the Settings (`widget.ID`), kept in config.toml's `[widget.ID]`, read as
`{config.file}`, its default while the config says nothing. The properties are the Settings schema's
(docs/plugins.md, Config): `type` (`string` if left out; `number` with `min`, `max`, `step`, `slider`; `bool`;
`choice` and `multi` with `options="a b c"`; `list`, `path`, `color`, `duration`, `url`), `title` (the key if
left out), `help`, `default`. A change in the config draws the widget again at once.

## More examples

The load averages, with the battery from ostrov's state:

```kdl
widget "cpu-load" name="CPU Load" icon="utilities-system-monitor-symbolic" sizes="4x1 2x1 8x1" {
    setting "file" title="Read from" type="path" default="/proc/loadavg"
    poll "load" every="5s" exec="cut -d' ' -f1-3 {config.file}"
    box orientation="horizontal" {
        image icon="utilities-system-monitor-symbolic"
        label text="{load}" class="bold"
        if "{state.battery.percent}" { label text="{state.battery.percent}%" class="dim" }
    }
}
```

The player, followed rather than polled:

```kdl
widget "player" name="Player" icon="multimedia-player-symbolic" sizes="4x1 8x1" {
    listen "track" exec="playerctl --follow metadata --format '{{{{artist}}}} – {{{{title}}}}'"
    listen "status" exec="playerctl --follow status"
    toggle icon="multimedia-player-symbolic" title="{track || 'Nothing playing'}" on="{status == 'Playing'}" {
        toggle exec="playerctl play-pause"
        menu {
            row icon="media-skip-forward-symbolic" text="Next" { click exec="playerctl next" }
        }
    }
    badge icon="multimedia-player-symbolic" text="{track}" show="active" active="{status == 'Playing'}"
}
```

Choosing an icon:

    toggle icon="{st.on && 'network-vpn-symbolic' || 'network-vpn-disabled-symbolic'}" title="VPN" on="{st.on}"
