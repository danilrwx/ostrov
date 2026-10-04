# Contributing to ostrov

Build and test with `cargo build --release` and `cargo test --release`; both must stay free of warnings. Run a
build beside your own ostrov without stopping it by giving it another application id:

    OSTROV_APP_ID=dev.ostrov.Test ./target/release/ostrov

## Style

- Match the code around you. Lines are at most 120 characters.
- Doc comments are plain prose saying what a thing is and why it is so, not restating the code.
- No `unwrap()` or `expect()` on paths that run at runtime: an error is returned, shown to the user or written to
  stderr as `ostrov: ...`. Tests may unwrap.
- No dead code and no TODOs; nothing of one machine or one person (paths, usernames, hardware) in the code.
- New crates only when they are truly needed.
- GTK runs on the main thread only; anything that blocks (D-Bus, processes, files, the network) goes to the
  services' Tokio runtime or a thread of its own.
- Colours are named tokens in `src/style.rs` (`@define-color`), the themes in `src/look.rs` setting them; the
  rest of the CSS uses the names.

## Where things go

- `src/main.rs`: what is built at start, and the dispatch of `ostrov ARGS` (`FORMS` lists ostrov's own commands in
  the grammar of `src/forms.rs`, which also drives the shells' completion).
- `src/modules/<id>/`: a part of the desktop, whole: `service.rs` (its state, its commands, its worker on the
  services' runtime) and `widget.rs` (its widgets). Its `mod.rs` declares a `Module` (id, command forms, state,
  run, worker, widgets, completion). **To add a module**, write one and list it in `ALL` in `src/modules/mod.rs`;
  nothing else in ostrov names it. Its state appears in `ostrov dump` under its id, its commands as `ostrov ID
  ARGS`.
- **To add a widget**, give a module a `WidgetDef` (id, gallery name, icon, the sizes it allows with the default
  first, the function that builds it, optionally a settings schema kept in `[widget.ID]`). It joins the panels'
  gallery.
- `src/bar/`: the bar's blocks, built by name in `src/bar/mod.rs`.
- `src/cc/`: the panels, Edit and the inspector. `src/settings/`: the schema-driven forms and secrets.
- `src/plugins/`, `src/widgets/`: plugins (processes, [docs/plugins.md](docs/plugins.md)) and KDL widgets
  ([docs/widgets.md](docs/widgets.md)). A widget that only runs commands is better as a KDL file or a plugin
  than as a module.
- `src/services/`: the runtime and the shared D-Bus, rfkill and location helpers.

## Commits

One change per commit, its message `ostrov: <what changed>`, in the style of the log.
