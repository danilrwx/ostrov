# ostrov plugins

A plugin is a program, in any language, that puts widgets on ostrov's control centre. It is written against one
protocol, `wit/ostrov-plugin.wit` (package `ostrov:plugin@1.0.0`, world `ostrov-plugin`): the plugin's
**exports**, called by ostrov, and ostrov's **imports**, called by the plugin. Today a plugin runs as a process
and the calls go as JSON lines over its stdin and stdout; a WebAssembly component could speak the same WIT later,
with the same behaviour.

Besides plugins, ostrov has a D-Bus face for scripts: see [D-Bus](#d-bus) at the end.

## Install

    ~/.local/share/ostrov/plugins/<id>/manifest.toml
    ~/.local/share/ostrov/plugins/<id>/...            # whatever exec runs

ostrov reads the plugins as it starts (restart it for a new one). Try the example:

    cp -rL examples/plugins/hello-python ~/.local/share/ostrov/plugins/

then **Edit** in the control centre, and pick "Hello Counter" from the gallery. `ostrov plugins` lists what was
found: the manifests, each plugin's state, its settings' schema. A manifest that does not read is said on ostrov's
stderr and skipped.

## Manifest

```toml
id = "hello-python"            # the directory's name, of [a-z0-9-]
name = "Hello"
version = "0.1.0"
api = 1                        # the protocol's version; another one is not run
exec = "python3 main.py"       # run through sh -c, in the plugin's directory
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
```

Unknown keys are ignored.

## Config

The plugin's settings are the `[plugin.<id>]` section of `~/.config/ostrov/config.toml`, handed to it by
`on-config` as it starts and again whenever the section changes:

```toml
[plugin.hello-python]
greeting = "Privet"
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
| *(instantiation)*                        | `{"type":"hello","api":1}`, first, once per start                           | –                                      |
| `on-config(json)`                        | `{"type":"on_config","json":{...}}`, right after hello, and on every change | –                                      |
| `state() -> string`                      | `{"type":"state","call":N}`                                                 | `return` with `value`: any JSON        |
| `run(args, input) -> result<string,string>` | `{"type":"run_request","id":N,"args":["set","5"],"input":null}`         | `{"type":"run_result","id":N,"ok":"5"}`/`{...,"err":"..."}` |
| `render(widget) -> string`               | `{"type":"render","call":N,"widget":"counter"}` (`"counter#bar"`: its badge) | `return` with a node tree, or `null`   |
| `on-event(widget, node, event, value)`   | `{"type":"on_event","widget":"counter","node":"count","event":"toggle","value":"true"}` | –                          |
| `on-timer(id)`                           | `{"type":"on_timer","id":7}`                                                | –                                      |
| `on-state(json)`                         | `{"type":"on_state","json":{...}}`, with permission `state`                 | –                                      |

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

## Permissions

Declared in the manifest, shown by `ostrov plugins`, and kept to where ostrov can:

| permission | gives                                                  | without it                           |
|------------|--------------------------------------------------------|--------------------------------------|
| `run`      | the `run` import, any `ostrov` command                 | `{"err":"not permitted: run"}`       |
| `network`  | the `http-get` import                                  | `{"err":"not permitted: network"}`   |
| `secrets`  | the `secret` import                                    | `null`                               |
| `dialogs`  | the `ask` import                                       | `null`                               |
| `state`    | `on_state`: the desktop's state (`ostrov dump`)        | never sent                           |

A process is not sandboxed: it can do whatever its user can, whatever its manifest says. Under WebAssembly the
permissions would be which imports the component is linked with.

## SDK

`sdk/python/ostrov_plugin.py`: subclass `Plugin`, override the exports, call `log`, `host_run`, `ask`, `secret`,
`http_get`, `set_timer`, `kick`, `set_settings_schema` (and `push_render`, `push_state`), run `main()`. See
`examples/plugins/hello-python/main.py`.

## D-Bus

ostrov owns `dev.ostrov.Shell` on the session bus, object `/dev/ostrov/Shell`, interface `dev.ostrov.Shell`:

| member                        | does                                                             |
|-------------------------------|------------------------------------------------------------------|
| `Run(as args) -> s`           | `ostrov ARGS` in the running ostrov: its output, or a D-Bus error |
| `State() -> s`                | the desktop's state as JSON (`ostrov dump`)                      |
| `OpenPanel(s menu)`           | the control centre, that widget's menu unfolded (`""` for none)  |
| `Toast(s title, s body)`      | a notification of ostrov's own (`ostrov toast TITLE BODY`)        |

`Run(["dialog", JSON])` asks a dialog and answers once it is answered (a no is a D-Bus error).
| signal `StateChanged(s state)`| the state, whenever it changes                                   |

    busctl --user call dev.ostrov.Shell /dev/ostrov/Shell dev.ostrov.Shell Run as 2 menu wifi
