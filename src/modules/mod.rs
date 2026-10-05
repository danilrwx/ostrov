//! ostrov's parts, each whole in its own directory: what it does off GTK's thread (its state, under its id in the
//! desktop's state; its commands, `ostrov ID ARGS`; its worker, kicking whenever its state is to be read again)
//! and what it puts on the screen (its widgets, for the panels' grids and their gallery). The rest of ostrov
//! knows none of them by name: the services' runtime (services/) goes through ALL for the state, the commands,
//! the workers; the control centre for its widgets. A plugin is one too, made of a process (plugins/).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

use crate::cc::{Ctx as Ui, Show, Widget};
use crate::services::{Ctx, Kick, Res};

pub mod airplane;
pub mod audio;
pub mod battery;
pub mod brightness;
pub mod bt;
pub mod calendar;
pub mod clock;
pub mod hyprland;
pub mod keymap;
pub mod location;
pub mod media;
pub mod power;
pub mod system;
pub mod sysinfo;
pub mod wallpaper;
pub mod weather;
pub mod wifi;

pub type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub struct Module {
    /// its key in the state, its command's first word
    pub id: &'static str,
    /// its commands' forms, the words after the id in forms.rs's grammar: for ostrov help, a command it does not
    /// know, the shells' completion
    pub forms: &'static [&'static str],
    pub state: Option<for<'a> fn(&'a Ctx) -> Fut<'a, Value>>,
    pub run: Option<for<'a> fn(&'a Ctx, Vec<String>, Option<String>) -> Fut<'a, Res>>,
    pub worker: Option<fn(Arc<Ctx>, Kick) -> Fut<'static, ()>>,
    pub widgets: &'static [WidgetDef],
    /// its placeholders' values for completion (an SSID, an ADDR), each with what it is, from its state
    pub complete: Option<fn(&Value, &str) -> Vec<(String, String)>>,
}

impl Module {
    /// A module of nothing yet, the base the others are written over.
    pub const NONE: Module =
        Module { id: "", forms: &[], state: None, run: None, worker: None, widgets: &[], complete: None };

    /// What a command it does not know gets.
    pub fn usage(&self) -> String {
        crate::forms::usage(self.id, self.forms)
    }
}

/// Values of a placeholder from a list in a module's state: each item's field `word` (the item itself if ""), what
/// it is by `about`.
pub fn values(list: &Value, word: &str, about: impl Fn(&Value) -> String) -> Vec<(String, String)> {
    let items = list.as_array().map(Vec::as_slice).unwrap_or_default();
    let word = |v: &Value| {
        let f = if word.is_empty() { v } else { &v[word] };
        f.as_str().map(String::from).or_else(|| f.as_u64().map(|n| n.to_string()))
    };
    items.iter().filter_map(|v| Some((word(v)?, about(v)))).collect()
}

/// A widget a module offers: its id, how the gallery shows it, the sizes it allows (the first its default), how
/// it is made, its settings' schema (kept in [widget.ID]).
pub struct WidgetDef {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub sizes: &'static [(u8, u8)],
    pub make: fn(&Ui) -> Widget,
    pub settings: Option<fn() -> crate::settings::Schema>,
    /// when its badge is in its panel's face in the bar, unless the panel says otherwise
    pub bar: Show,
}

impl WidgetDef {
    /// Its badge in the bar as bar says.
    pub const fn bar(mut self, bar: Show) -> WidgetDef {
        self.bar = bar;
        self
    }

    pub const fn settings(mut self, schema: fn() -> crate::settings::Schema) -> WidgetDef {
        self.settings = Some(schema);
        self
    }
}

/// A widget without settings, its badge (if it has one) not in the bar.
pub const fn widget(id: &'static str, name: &'static str, icon: &'static str, sizes: &'static [(u8, u8)], make: fn(&Ui) -> Widget) -> WidgetDef {
    WidgetDef { id, name, icon, sizes, make, settings: None, bar: Show::Never }
}

/// The sizes of a toggle: from a square to the whole width; of a round button; of a slider.
pub const TOGGLE: &[(u8, u8)] = &[(4, 1), (2, 1), (1, 1), (8, 1)];
pub const BUTTON: &[(u8, u8)] = &[(1, 1), (2, 1)];
pub const SLIDER: &[(u8, u8)] = &[(8, 1), (4, 1)];

pub const ALL: &[&Module] = &[
    &wifi::MODULE,
    &bt::MODULE,
    &airplane::MODULE,
    &audio::MODULE,
    &audio::HEADSET,
    &power::MODULE,
    &battery::MODULE,
    &brightness::MODULE,
    &location::MODULE,
    &keymap::MODULE,
    &media::MODULE,
    &wallpaper::MODULE,
    &weather::MODULE,
    &calendar::MODULE,
    &clock::MODULE,
    &hyprland::MODULE,
    &system::MODULE,
    &sysinfo::MODULE,
];

/// A command's words as the services take them.
pub fn words(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}
