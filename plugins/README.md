# ostrov's official plugins

Each directory here is one plugin ostrov ships, written in Rust against the SDK in `../sdk` (the crate
`ostrov-plugin`; docs/plugins.md, "Writing a plugin in Rust") and built with ostrov by
`cargo build --release --workspace`. It runs as any plugin does, a process speaking JSON lines, and is off until
the user says `[plugin.<id>] enabled = true` (docs/plugins.md, "Official plugins"). `hello` is the SDK's example,
not shipped.

## Adding one

    plugins/<id>/Cargo.toml        the crate ostrov-plugin-<id>, its binary of the same name
    plugins/<id>/manifest.toml     exec = "ostrov-plugin-<id>"
    plugins/<id>/i18n/ru.toml      its texts in Russian, the manifest's too
    plugins/<id>/src/main.rs       impl Plugin, ostrov_plugin::run(...)

`<id>` is of `[a-z0-9-]`, and is its directory's name, its config's section (`[plugin.<id>]`) and its commands'
word (`ostrov plugin <id> ...`). The workspace takes the crate in by itself (`members = ["sdk", "plugins/*"]`);
ostrov's own `[dependencies]` stay as they are.

`Cargo.toml`, versions of the crates the workspace's lock already has where one serves (serde, serde_json, toml,
ureq...), so nothing is built twice:

```toml
[package]
name = "ostrov-plugin-<id>"
version = "0.1.0"
edition = "2024"
rust-version = "1.93"
description = "ostrov's <what it is>"
license = "MIT"
publish = false

[dependencies]
ostrov-plugin = { path = "../../sdk" }
```

`manifest.toml` (docs/plugins.md, "Manifest"), its texts in English:

```toml
id = "<id>"
name = "Night Light"
version = "0.1.0"
api = 1
exec = "ostrov-plugin-<id>"    # found on PATH, else beside ostrov's own binary
icon = "night-light-symbolic"
description = "Warmer colours in the evening."
permissions = ["run"]          # only what it uses: run, dialogs, network, secrets, state, events, keys, calendar

[[widgets]]
id = "toggle"
name = "Night Light"
sizes = [[4, 1], [2, 1]]

[[commands]]
name = "toggle"
help = "the night light on or off"
```

`config` and `enabled`: its settings are `[plugin.<id>]`, read with `host.config()` / `host.setting::<T>(key)`,
handed again to `on_config` as they change; their form is `host.set_settings_schema(...)`, sent from
`on_config` (see `hello`). `enabled` in that section is ostrov's, not the plugin's.

## Its texts

Every text the user sees is written in English and passed through `ostrov_plugin::t("...")` (`fill(t("{} min"),
&[&n])` for one with values) as it is shown; its Russian goes in `i18n/ru.toml`, `"English" = "Русский"` lines as
ostrov's own `i18n/ru/*.toml`. The SDK reads the file named by the language ostrov sends in hello, from the
plugin's working directory (its manifest's). ostrov reads the same file for the manifest's `name`,
`description`, widgets' and launcher modes' names and commands' `help`, so those go in it too.

## What waits

The plugin's methods run one at a time on its thread; ostrov waits 10 s for a call's answer. Anything that may
take longer (a dialog, the network, a child process, the keyring) runs on a thread of its own with a clone of the
`Host`, sends what it found over a channel, and wakes the plugin with `host.set_timer(0, WAKE)`; `on_timer`
takes it in and `host.kick()`s. `hello`'s rename and token do this.

## Shipping it

- `Cargo.toml` (root), `[package.metadata.deb]`'s assets, three lines:
  `["target/release/ostrov-plugin-<id>", "usr/bin/", "755"]`,
  `["plugins/<id>/manifest.toml", "usr/share/ostrov/plugins/<id>/", "644"]`,
  `["plugins/<id>/i18n/*", "usr/share/ostrov/plugins/<id>/i18n/", "644"]`; the programs it runs in `recommends`.
- `packaging/arch/PKGBUILD` and `flake.nix` install every `plugins/*/` but `hello` by themselves; the programs
  it runs go in `optdepends`, and in the flake's `gappsWrapperArgs` PATH.

## Trying it

    cargo build --release --workspace
    mkdir -p /tmp/share/ostrov/plugins && cp -r plugins/<id> /tmp/share/ostrov/plugins/
    XDG_DATA_DIRS=/tmp/share:/usr/local/share:/usr/share OSTROV_APP_ID=dev.ostrov.Test ./target/release/ostrov

with `[plugin.<id>] enabled = true` in the config: the binary is found beside `target/release/ostrov` (one of
the same name on PATH comes first). A headless one, not on your screen: `HOME` a directory of its own,
`WAYLAND_DISPLAY` and `HYPRLAND_INSTANCE_SIGNATURE` unset, `WLR_BACKENDS=headless dbus-run-session -- cage --
script.sh`, the script starting ostrov in the background and running `ostrov plugin <id> ...`, `ostrov panel`, `ostrov capture FILE` with its stdin `</dev/null` (a command
of a plugin's reads its stdin when it is not a terminal).
