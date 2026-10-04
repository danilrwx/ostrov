# ostrov plugins

A plugin is a program, in any language, that puts widgets on ostrov's control centre, or modes in its launcher. It is written against one
protocol, `wit/ostrov-plugin.wit` (package `ostrov:plugin@1.0.0`, world `ostrov-plugin`): the plugin's
**exports**, called by ostrov, and ostrov's **imports**, called by the plugin. Today a plugin runs as a process
and the calls go as JSON lines over its stdin and stdout; a WebAssembly component could speak the same WIT later,
with the same behaviour.

Besides plugins, ostrov has a D-Bus face for scripts: see [D-Bus](#d-bus) at the end.

## Install

    ~/.local/share/ostrov/plugins/<id>/manifest.toml
    ~/.local/share/ostrov/plugins/<id>/...            # whatever exec runs

ostrov reads the plugins as it starts. Try the example:

    ostrov plugin install examples/plugins/hello-python

`ostrov plugin install PATH|GIT-URL` copies a plugin's directory there, its symlinks resolved (the examples link
the SDK), or `git clone --depth 1`s a URL first (`https://...`, `git@host:path`), its `.git` left behind. The
manifest is checked, and a dialog says what its permissions let it do before anything is copied; a no installs
nothing. The plugin starts at once: its commands, keys, launcher modes, events and calendar work without a
restart, its widgets join the gallery after `ostrov restart`. Installed again over one that runs, it is replaced
on disk and runs as new after `ostrov restart`. `ostrov plugin remove ID` ends its process, takes its keys, modes
and calendar away and deletes its directory (its widgets and settings leave at the next restart). Copying the
directory by hand works too (`cp -rL`), read at the next start.

Then **Edit** in the control centre, and pick "Hello Counter" from the gallery. `ostrov plugins` lists what was
found: the manifests, each plugin's state, its settings' schema, whether it is official and enabled. A manifest
that does not read is said on ostrov's stderr and skipped. `install` and `remove` are no plugin's id.

Any plugin is turned off with `enabled = false` in its `[plugin.<id>]` section (see Config), and on again with
`true` or the line gone: it stops or starts as the config is saved, its widgets leaving or joining the gallery at
the next `ostrov restart`. [Official plugins](#official-plugins) are off until `enabled = true`.

## Manifest

```toml
id = "hello-python"            # the directory's name, of [a-z0-9-]
name = "Hello"
version = "0.1.0"
api = 1                        # the protocol's version; another one is not run
exec = "python3 main.py"       # run through sh -c, in the plugin's directory, ostrov's own directory last in PATH
icon = "face-smile-symbolic"   # an icon name of the theme
description = "Counts its clicks."
permissions = ["secrets"]      # see Permissions

[[widgets]]
id = "counter"                 # on the grid as plugin.<id>.<widget id>: plugin.hello-python.counter
name = "Hello Counter"         # in the gallery, and its menu's title
icon = "face-smile-symbolic"   # the plugin's if left out
sizes = [[4, 1], [2, 1], [8, 1]]  # cells [w, h] the grid (8 wide) allows, the first the default; [[4, 1]] if none
badge = true                   # a badge in its panel's face in the bar (see Badges); false if left out
bar = "active"                 # when the badge shows: always, active (while it says so), never (the default)

[[launcher]]
prefix = "?"                   # what is typed starting with it is the plugin's (see Launcher modes)
name = "Ask"                   # the launcher's prompt while in it
icon = "dialog-question-symbolic"

events = ["window", "power"]   # ostrov's events it is sent, "*" for all (permission events; see Events)
calendar = true                # a calendar of the calendar's (permission calendar; see Calendars)

[[keys]]
combo = "SUPER, F12"           # as Hyprland's binds write it: "MODS, KEY" (see Keys)
command = "toggle"             # its own command's words: `ostrov plugin <id> toggle`
```

Unknown keys are ignored. Its texts (`name`, `description`, the widgets' and launcher modes' `name`, the
commands' `help`) are shown in the user's language when the plugin has a catalogue for it (see
[Texts](#texts)).

## Config

The plugin's settings are the `[plugin.<id>]` section of `~/.config/ostrov/config.toml`, handed to it by
`on-config` as it starts and again whenever the section changes:

```toml
[plugin.hello-python]
greeting = "Privet"
enabled = true                 # ostrov's own: whether it runs (see Install), handed to it with the rest
```

A plugin describes its settings' form with `set-settings-schema`: the Settings page's own schema (`src/settings`),
as JSON. It shows as an entry of the Settings page under the plugin's name and icon, and writes `[plugin.<id>]`:

```json
{"sections": [{"title": "Hello", "fields": [
  {"key": "greeting", "title": "Greeting", "type": "string", "default": "Hello"},
  {"key": "token", "title": "API token", "type": "secret", "actions": [{"label": "Test", "id": "test-token"}]}
]}]}
```

Field types: `string`, `number` (`min`, `max`, `step`, `slider`), `bool`, `choice` and `multi` (`options`: strings
or `{value, label}`), `list`, `secret`, `path`, `color`, `duration`, `url`; any field may have `help`, `default`,
`visible_if` (`{key, equals}`) and `actions`. An action's button runs the plugin's command `action <id>`, the
field's value as its input, and shows its `ok` or `err` under the field.

A `secret` field never goes into config.toml: the Settings page keeps it in the Secret Service (attributes
`app=ostrov`, `key=plugin.<id>.<key>`, or ~/.local/share/ostrov/secrets.toml without one), and the plugin reads
it with `secret(key)`, permission `secrets`.

## The protocol

Every message is one JSON object on one line. Its `"type"` is the WIT function's name in snake_case; its fields
are the function's parameters, by the same names. A call that answers carries a `"call"` number, and its answer
comes back as `{"type": "return", "call": N, "value": ...}` (or `"error": "..."` instead of `"value"` when the
callee failed). Numbers are each side's own: ostrov numbers its calls, the plugin its own. Several calls may be in
flight at once, answered in any order. The one exception is the `run` export, the command line's way in: it goes
as `run_request` with an `"id"`, and comes back as `run_result` with the same `"id"`.

ostrov waits 10 s for an answer, then fails the call. A call to a plugin that is starting again waits for it up
to 2 s, then fails with "plugin ID not running".

Two systematic differences from the WIT:

- a parameter or result the WIT types `json: string` is the JSON value itself in the message, not text;
- a WIT `result<T, E>` is `{"ok": T}` or `{"err": E}`, an `option<T>` is the value or `null`, a `list<u8>` an
  array of numbers.

### Exports: ostrov calls the plugin

| WIT (`interface plugin`)                 | JSON line ostrov writes                                                     | the plugin answers                     |
|------------------------------------------|-----------------------------------------------------------------------------|----------------------------------------|
| *(instantiation)*                        | `{"type":"hello","api":1,"language":"ru"}`, first, once per start (see Texts) | –                                    |
| `on-config(json)`                        | `{"type":"on_config","json":{...}}`, right after hello, and on every change | –                                      |
| `state() -> string`                      | `{"type":"state","call":N}`                                                 | `return` with `value`: any JSON        |
| `run(args, input) -> result<string,string>` | `{"type":"run_request","id":N,"args":["set","5"],"input":null}`         | `{"type":"run_result","id":N,"ok":"5"}`/`{...,"err":"..."}` |
| `render(widget) -> string`               | `{"type":"render","call":N,"widget":"counter"}` (`"counter#bar"`: its badge) | `return` with a node tree, or `null`   |
| `on-event(widget, node, event, value)`   | `{"type":"on_event","widget":"counter","node":"count","event":"toggle","value":"true"}` | –                          |
| `on-timer(id)`                           | `{"type":"on_timer","id":7}`                                                | –                                      |
| `on-state(json)`                         | `{"type":"on_state","json":{...}}`, with permission `state`                 | –                                      |
| `query(mode, text) -> string`            | `{"type":"query","call":N,"mode":"?","text":"why"}`                         | `return` with a list of hits, or `null` |
| `pick(mode, id, text)`                   | `{"type":"pick","mode":"?","id":"1","text":"why"}`                          | –                                      |
| `on-shell-event(name, json)`             | `{"type":"on_shell_event","name":"window","json":{"class":"foot","title":"~"}}`, with permission `events` | – |
| `calendar-events(from, to) -> string`    | `{"type":"calendar_events","call":N,"from":"2026-09-24T00:00:00","to":"2026-11-08T00:00:00"}`, with permission `calendar` | `return` with a list of events, or `null` |

### Imports: the plugin calls ostrov

| WIT (`interface host`)                     | JSON line the plugin writes                                    | ostrov answers                            |
|--------------------------------------------|----------------------------------------------------------------|-------------------------------------------|
| `log(msg)`                                 | `{"type":"log","msg":"..."}`                                   | –                                         |
| `run(args) -> result<string,string>`       | `{"type":"run","call":N,"args":["menu","wifi"]}`               | `return`, `{"ok":"..."}`/`{"err":"..."}`  |
| `ask(dialog) -> option<string>`            | `{"type":"ask","call":N,"dialog":{"kind":"text",...}}`         | `return`, the answer or `null` (see Dialogs) |
| `secret(key) -> option<string>`            | `{"type":"secret","call":N,"key":"token"}` (its `plugin.<id>.token`) | `return`, a string or `null`        |
| `http-get(url) -> result<list<u8>,string>` | `{"type":"http_get","call":N,"url":"https://..."}`             | `return`, `{"ok":[bytes]}`/`{"err":"..."}` |
| `set-timer(ms, id)`                        | `{"type":"set_timer","ms":1000,"id":7}`                        | `on_timer` once, ms later                 |
| `kick()`                                   | `{"type":"kick"}`                                              | ostrov calls `state`, then `render` for each widget |
| `set-settings-schema(json)`                | `{"type":"set_settings_schema","json":{...}}`                  | –                                         |

### Pull and push

The one place the transports may differ. A WebAssembly plugin can only be pulled: it calls `kick()` when
something changed, and ostrov calls `state()` and `render(widget)` for each of its widgets. A process plugin may
do the same (and should: it behaves identically under either transport), or push unasked:

| push (process only)                                   | is the same as                       |
|-------------------------------------------------------|--------------------------------------|
| `{"type":"render","widget":"counter","tree":NODE}`    | `render("counter")` returning NODE   |
| `{"type":"state","json":{...}}`                       | `state()` returning it               |

ostrov kicks itself once a plugin has started (after hello and on_config), so a fresh plugin is drawn at once.
`render` returning `null` keeps what is drawn.

### Lifetime

ostrov starts the plugin as it starts, and again whenever it ends: 1 s later, doubling each time it ends within a
minute of starting, up to a minute. The plugin should exit when its stdin closes (ostrov is gone). Everything it
writes on stderr goes to ostrov's, `ostrov: plugin <id>:` before it; nothing but protocol may go to stdout. It
must answer every call it is sent (an `error` return is fine).

## Commands

`ostrov plugin <id> ARGS` calls the plugin's `run(ARGS, input)` and prints its answer: `ok` on stdout, `err` on
stderr with exit status 1. What is piped to `ostrov` is `input` (`echo text | ostrov plugin hello-python note`);
`null` from a terminal or when nothing is piped. D-Bus `Run(["plugin", id, ...])` does the same, with no input.

The manifest lists the commands for help and the shells' completion; the plugin reads its words itself:

```toml
[[commands]]
name = "set"
usage = "set N"               # its words after the id; its name if left out
help = "the count set to N"
```

`usage` is in the grammar of ostrov's own commands (`src/forms.rs`): words separated by spaces, `a|b|c`
alternatives at one place, an UPPERCASE word a placeholder, `[X]` optional, a trailing `...` repeating
(`mode on|off|auto`, `note [TEXT...]`). The words complete in the shell (`ostrov completions zsh|bash|fish`),
`help` as their description; placeholders complete to nothing.

`ostrov plugin <id>` or `ostrov plugin <id> help` prints them, `ostrov plugin` every plugin's, `ostrov help`
ostrov's own and every plugin's.

## Launcher modes

A `[[launcher]]` of the manifest is a mode of the launcher (`ostrov run`): what is typed starting with its
`prefix` goes to the plugin, its `name` the prompt. As the user types (once typing pauses, 150 ms), ostrov calls
`query(mode, text)`, `mode` the prefix and `text` what follows it; an answer later than 2 s, or to text since
changed, is dropped. The answer is the launcher's rows:

```json
[{"id": "1", "text": "Ask: why", "note": "claude.ai", "icon": "dialog-question-symbolic",
  "open": "https://example.org/?q=why"}]
```

Every field but `text` may be left out. A row picked (Enter or a click) with `open` has ostrov open that URI in
its default app, with `copy` put that text on the clipboard; any other calls `pick(mode, id, text)`. So a plugin
whose rows open or copy needs no `pick`.

The launcher's own prefixes come first, and a plugin's may not overlap them (`:` emoji, `s ` web search, `/`
files), nor another plugin's (the first by id keeps it): neither may start with the other. A prefix that may not
is said on ostrov's stderr and left out. A prefix may end in a space (`g `), so `gimp` still runs as an app. A
mode comes before the calculator and the apps: a prefix of letters takes what is typed from them.

## Events

A plugin with permission `events` is sent `on_shell_event(name, json)` for each of ostrov's events its manifest's
`events` lists (`["*"]`: all of them), the ones D-Bus's `Event` signal carries (src/events.rs):

| name        | payload                                   | when                                        |
|-------------|-------------------------------------------|---------------------------------------------|
| `window`    | `{"class", "title"}` (`""` for none)      | a window focused                            |
| `workspace` | `{}`                                      | a workspace made, gone, focused             |
| `lock`, `unlock` | `{}`                                 | the screen locked, unlocked                 |
| `wallpaper` | `{"on", "path"}`                          | the wallpaper picked                        |
| `network`   | `{"on", "ssid"}`                          | Wi-Fi on or off, a network joined or left   |
| `power`     | `{"state", "plugged", "percent"}`         | the battery charging, discharging, full     |
| `bluetooth` | `{"on", "connected"}`                     | Bluetooth on or off, a device connected     |
| `output`    | `{"name"}`                                | the default sound output                    |
| `media`     | `{"playing", "title", "artist"}`          | the player playing or not, a new track      |

Without the permission, or with an empty `events`, none is sent.

## Keys

A `[[keys]]` of the manifest (permission `keys`) is a key in Hyprland bound to the plugin's own command: `combo`
as Hyprland's binds write it (`"SUPER, F12"`, `"SUPER SHIFT, K"`, `", XF86Launch1"`), `command` the words after
`ostrov plugin <id>`. ostrov binds it as it binds its own keys: only where the combination is free (a bind of the
user's, ostrov's own and an earlier plugin's, by id, come first), as the plugin starts, and again after every
reload of Hyprland's config; removed, the plugin's binds go. `ostrov hyprland` prints them as hyprland.conf lines
with ostrov's own, `ostrov doctor` says which are bound and which taken by something else. `[hyprland] binds =
false` binds none.

## Calendars

A plugin whose manifest says `calendar = true` (permission `calendar`) is one of the calendar's calendars, beside
`[calendar]`'s CalDAV and .ics ones. As the calendar fetches (as ostrov starts, every 15 min, on `ostrov calendar
refresh`, and when the plugin starts), ostrov calls `calendar_events(from, to)`, the span it shows (this month and
a week either side) as local ISO times, and the plugin answers with its events in it:

```json
[{"title": "Lunch", "start": "2026-10-05T12:00:00", "end": "2026-10-05T13:00:00", "all_day": false,
  "location": "the kitchen", "color": "#e5a50a"}]
```

Times are local, ISO 8601 without a zone, as the calendar's own; a day's event starts at its day's midnight and
ends at the next, `all_day` true. `title` and `start` are needed (an event without them is dropped); `end` is the
start if left out, `all_day` false, `location` and `color` empty (the theme's). The events are merged with the
other calendars' and sorted by their starts. A plugin that fails or does not answer in 10 s is said on stderr,
the others' events kept. See `examples/plugins/calendar-demo`.

## Nodes

A widget is a tree of nodes, ostrov's own kit (`src/ui.rs`), so a plugin's widget looks like the built-in ones.
Every node has a `"type"`, and may have an `"id"`: a node with one sends `on_event` as it is used. Fields left out
take their defaults (empty, false, 0).

| type        | fields                                              | events (`value`)                            |
|-------------|-----------------------------------------------------|---------------------------------------------|
| `toggle`    | `icon`, `title`, `sub`, `on`, `menu` (a node)       | `toggle` (`"true"`/`"false"`: the new state) |
| `slider`    | `icon`, `value` (0 to 1)                            | `change` (the value, `"0.42"`)              |
| `button`    | `icon`, `label`                                     | `click` (`""`)                              |
| `round`     | `icon`, `label` (its tooltip)                       | `click`                                     |
| `label`     | `text`, `class` (`dim`, `bold`, `title`, `error`)   | –                                           |
| `row`       | `icon`, `text`, `note`, `on` (ticked)               | `click`                                     |
| `box`       | `orientation` (`vertical`, `horizontal`), `children` | –                                          |
| `image`     | `icon`, or `path` (absolute)                        | –                                           |
| `progress`  | `value` (0 to 1)                                    | –                                           |
| `chips`     | `options` (strings), `on` (an index)                | `change` (the index picked, `"1"`)          |
| `entry`     | `placeholder`                                       | `change` (its text, on Enter)               |
| `separator` |                                                     | –                                           |

The widget's root is normally a `toggle`; its `menu` unfolds under the grid's rows when its arrow is clicked, in a
card titled with the widget's name. A root other than a toggle, a slider or a round button sits on a card.
A toggle narrower than four cells is its icon alone.

A tree like the one drawn before but in its values (a toggle's `on`, `sub`, `icon`; a slider's `value`; a label's
`text`; a progress's `value`) updates the widgets in place, so a slider being dragged or an entry being typed into
is not disturbed; anything else redraws the widget.

## Badges

A widget whose manifest says `badge = true` has a badge in its panel's face in the bar, as the built-in widgets
do (a VPN's while it is up). ostrov renders it as one more widget, `render("<widget>#bar")`, after the widget's
own render on every kick (or pushed: `{"type":"render","widget":"counter#bar","tree":...}`). Its tree is a small
one, an `image` and a `label`, or a horizontal `box` of them, with one field more at its top, `"active"`:

```json
{"type": "box", "orientation": "horizontal", "active": true, "children": [
  {"type": "image", "icon": "face-smile-symbolic"}, {"type": "label", "text": "12"}
]}
```

When it shows is the manifest's `bar` unless the panel's Edit says otherwise (its eye): `always`, `active` (while
the last badge rendered says `"active": true`), `never`. Until the plugin renders it, the badge is the widget's
icon, inactive. A plugin that does not declare `badge` is never asked for `#bar`, so `api = 1` plugins written
before badges work unchanged.

## Dialogs

ostrov's dialog is the one polkit and ssh's askpass use: the screen dimmed, a card in its middle, the keyboard
its own (Tab and Shift+Tab around, Enter, Escape for a no). One shows at a time; the next waits. A plugin asks
with `ask(dialog)` (permission `dialogs`), a script with `ostrov dialog JSON`, which prints the answer and exits 1
on a no:

    ostrov dialog '{"kind": "choice", "title": "Profile", "options": ["work", "home"]}'

The spec, every field but `kind` optional:

| field                 | for                                                                 |
|-----------------------|---------------------------------------------------------------------|
| `kind`                | `confirm`, `text`, `secret`, `choice`, `form`                       |
| `icon`, `title`, `text` | the card's icon, title and words                                  |
| `error`               | said in red under it (what went wrong last time)                    |
| `ok`, `cancel`        | the buttons' words (OK, Cancel)                                     |
| `placeholder`, `value` | `text`: the entry's placeholder and what it starts as              |
| `options`             | `choice`: strings, a row each; a click on one is the answer         |
| `schema`              | `form`: a settings schema (see Config), drawn by the Settings page's form engine |

The answer: `""` for a confirm's yes, the text, the password, the option picked, or for a form a JSON object of
its keys (every section's in one object), secrets included. A form's answers are written nowhere: not config.toml,
not the keyring. A no is `null` (exit 1 for `ostrov dialog`).

A plugin's dialog always says who asks, its icon and id above it all ("hello-python asks"), so it cannot pass for
polkit or ostrov. A plugin has one dialog asked at a time (another is `null` at once), taken back as a no after 5
minutes. `ask` waits for the user, and the plugin's `run` is answered within 10 s: a command that asks answers at
once and asks later (`set_timer(0, id)`, then `ask` from `on_timer`; see the example's `rename`).

## Versioning

`api = 1` is this protocol. Within it, both sides ignore what they do not know: unknown fields, unknown message
types, unknown node types (drawn as nothing). New things come as new optional fields, new messages, new node types;
a change that breaks plugins is a new `api`, and a plugin asking for an API ostrov does not speak is not run.
Launcher modes (`[[launcher]]`, `query`, `pick`) came so, within `api = 1`, and so did events (`events`,
`on_shell_event`), keys (`[[keys]]`) and calendars (`calendar`, `calendar_events`): a plugin written before them
is never sent them, and an older ostrov ignores them in a manifest.

## Texts

A plugin shows its texts in the user's language the way ostrov does: written in English, looked up as they are
shown in a catalogue of the language's, `i18n/<lang>.toml` in the plugin's directory (its working directory), of
`"English" = "theirs"` lines as ostrov's own `i18n/` are:

```toml
# Russian: "English" = "Русский"
"{} clicks" = "{} нажатий"
"Hello Counter" = "Счётчик Привет"
```

`hello`'s `language` is the language ostrov shows itself in, two letters (`[appearance] language`, else the
locale's): the Rust SDK's `t()` and `fill()` look the plugin's texts up in it. ostrov reads the same catalogue for
the manifest's texts as it reads the manifest. A text with values in it says `{}` for each, so a language may move
them; a text the catalogue lacks shows in English.

## Permissions

Declared in the manifest, shown by `ostrov plugins`, and kept to where ostrov can:

| permission | gives                                                  | without it                           |
|------------|--------------------------------------------------------|--------------------------------------|
| `run`      | the `run` import, any `ostrov` command                 | `{"err":"not permitted: run"}`       |
| `network`  | the `http-get` import                                  | `{"err":"not permitted: network"}`   |
| `secrets`  | the `secret` import                                    | `null`                               |
| `dialogs`  | the `ask` import                                       | `null`                               |
| `state`    | `on_state`: the desktop's state (`ostrov dump`)        | never sent                           |
| `events`   | `on_shell_event`: the events its manifest lists        | never sent                           |
| `keys`     | its `[[keys]]` bound in Hyprland                       | not bound                            |
| `calendar` | `calendar_events`: its events in the calendar          | never called                         |

`ostrov plugin install` shows them in its dialog before it installs anything.

A process is not sandboxed: it can do whatever its user can, whatever its manifest says. Under WebAssembly the
permissions would be which imports the component is linked with.

## SDK

`sdk/python/ostrov_plugin.py`: subclass `Plugin`, override the exports, call `log`, `host_run`, `ask`, `secret`,
`http_get`, `set_timer`, `kick`, `set_settings_schema` (and `push_render`, `push_state`), run `main()`. See
`examples/plugins/hello-python/main.py` (with an event, `on_shell_event`, and a key); a launcher mode in
`examples/plugins/claude/main.py` and `examples/plugins/google/main.py`; a calendar, `calendar_events`, in
`examples/plugins/calendar-demo/main.py`.

## Writing a plugin in Rust

The crate `ostrov-plugin` (`sdk/`, the workspace's) is the protocol in Rust, serde's and nothing async: implement
its `Plugin` trait, every method of which answers as nothing unless written, and hand it to `run`:

```rust
use ostrov_plugin::{json, Host, Plugin, Value};

struct Hello { on: bool }

impl Plugin for Hello {
    fn render(&mut self, _: &Host, _widget: &str) -> Option<Value> {
        Some(json!({"type": "toggle", "id": "t", "title": ostrov_plugin::t("Hello"), "on": self.on}))
    }
    fn on_event(&mut self, host: &Host, _widget: &str, _node: &str, _event: &str, value: &str) {
        self.on = value == "true";
        host.kick();
    }
}

fn main() { ostrov_plugin::run(Hello { on: false }) }
```

- The exports, `Plugin`'s methods, each handed the `Host`: `state`, `run(args: &[&str], input)` (match its words:
  `["set", n] => ...`), `render` (a node tree as `serde_json::Value`, `None` to keep what is drawn), `on_event`,
  `on_timer`, `on_config`, `on_state`, `query` (`Vec<Hit>`), `pick`, `on_shell_event`, `calendar_events`
  (`Result<Vec<CalendarEvent>, String>`). They run on the plugin's thread one at a time, in the order they come.
- The imports, `Host`'s: `log`, `run(&["menu", "wifi"])`, `toast(title, body)` (ostrov's `toast`, permission
  "run"), `ask(&Dialog)`, `secret(key)`, `http_get(url)`, `set_timer(ms, id)`, `kick`, `set_settings_schema`,
  `push_render`, `push_state`; its config, `config()` and `setting::<T>(key)`.
- A thread of the plugin's reads stdin, so the imports that answer (`ask`, `secret`, `http_get`, `run`) may be
  called from any thread: a `Host` is `Clone + Send`. Called from a method they hold ostrov's other calls back
  while they wait, so what waits long (a dialog, the network, the keyring being unlocked) is better done on a
  thread of its own, which sends what it found over a channel and wakes the plugin with `host.set_timer(0, ID)`:
  its `on_timer(ID)` takes it in on the plugin's thread. See `plugins/hello`.
- Every message is a type too, `FromOstrov` and `ToOstrov` (serde, `"type"`-tagged), with `Dialog`
  (`DialogKind`), `Hit` and `CalendarEvent`; `serve(plugin, input, output)` serves any lines, for tests.
- `t("English")`, `fill(text, &[&value])` and `language()`: its texts in the user's language (see Texts).

`plugins/hello` is the example: `hello-python`'s twin in Rust.

## Official plugins

ostrov's own plugins live in its repository's `plugins/<id>/`, each a crate of the workspace
(`ostrov-plugin-<id>`, built with `cargo build --release --workspace`) beside its `manifest.toml` and
`i18n/`; `plugins/README.md` says how to add one. The packages install a binary beside ostrov's
(`/usr/bin/ostrov-plugin-<id>`), the manifest saying `exec = "ostrov-plugin-<id>"`, and the manifest and texts in
`/usr/share/ostrov/plugins/<id>/`. ostrov reads official plugins from `share/ostrov/plugins/` beside its
executable's directory (`/usr/bin`'s `/usr/share`, a Nix store path's), then from each of `XDG_DATA_DIRS`
(`/usr/local/share:/usr/share` if unset), the first by an id kept; a plugin of the user's in
`~/.local/share/ostrov/plugins/` by the same id is read instead. Its binary is found as any exec is, ostrov's own
directory last in its PATH.

An official plugin is off until the user turns it on:

```toml
[plugin.<id>]
enabled = true
```

It starts as the config is saved (its widgets join the gallery after `ostrov restart`), and `enabled = false`
stops it again. `ostrov plugins` lists the official plugins found, `"official": true`, those off with
`"enabled": false`. `ostrov plugin remove` leaves an official plugin be, saying how to turn it off.

### games

While a window of one of its `classes` has the focus (a trailing `*` a prefix: `steam_app_*` any of Steam's), the
power profile is its `profile` (`performance` by default), set with `powerprofilesctl` (power-profiles-daemon); the
one before is put back as the focus leaves, unless it was picked by hand while the game played. It follows the
`window` event; no widget, no command. ostrov's old `[games]` section is these keys: move them to
`[plugin.games]` beside `enabled = true`. Notifications keep quiet in games by a list of their own,
`[notifications] games`, the same globs; copy the classes there too to keep that.

```toml
[plugin.games]
enabled = true
classes = ["dota2", "cs2", "steam_app_*"]
profile = "performance"   # performance, balanced, power-saver

[notifications]
games = ["dota2", "cs2", "steam_app_*"]
```

### record

Screen recording (was `ostrov record`): `ostrov plugin record toggle [--audio]`, SUPER SHIFT+R, a
  region picked with ostrov's selector (`ostrov pick-region`, which prints it as slurp does) recorded by
  `wf-recorder` to ~/Videos/Recordings/DATE_TIME.mp4 until asked again, `--audio` with what the default sink plays;
  its path then on the clipboard (`wl-copy`) and in a toast. Its widget `plugin.record.record` has a badge, a red
  dot and the time gone by, while recording: the bar's old `"record"` block is `"widget.plugin.record.record"` now,
  or the widget is put in a panel. `[hyprland.keys] record` is gone with it; a bind of the user's on SUPER SHIFT+R
  wins over the plugin's.

## D-Bus

ostrov owns `dev.ostrov.Shell` on the session bus, object `/dev/ostrov/Shell`, interface `dev.ostrov.Shell`:

| member                        | does                                                             |
|-------------------------------|------------------------------------------------------------------|
| `Run(as args) -> s`           | `ostrov ARGS` in the running ostrov: its output, or a D-Bus error |
| `State() -> s`                | the desktop's state as JSON (`ostrov dump`)                      |
| `OpenPanel(s menu)`           | the control centre, that widget's menu unfolded (`""` for none)  |
| `Toast(s title, s body)`      | a notification of ostrov's own (`ostrov toast TITLE BODY`)        |
| signal `StateChanged(s state)`| the state, whenever it changes                                   |
| signal `Event(s name, s payload)` | each of ostrov's events (src/events.rs: window, workspace, lock, unlock, wallpaper, network, power, bluetooth, output, media), its payload as JSON |

`Run(["dialog", JSON])` asks a dialog and answers once it is answered (a no is a D-Bus error).

    busctl --user call dev.ostrov.Shell /dev/ostrov/Shell dev.ostrov.Shell Run as 2 menu wifi
