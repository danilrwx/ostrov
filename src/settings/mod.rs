//! Settings as schemas: a schema is sections of fields (a string, a number in a range, a switch, one or more of
//! some options, a list, a secret, a path, a colour, a duration, a URL), each with its key, title, help and
//! default, shown only while another key has some value if it says so, with buttons of its own ("Test"). The
//! control centre's Settings page draws a form from each (form.rs), checks what is typed and writes it back to
//! config.toml in place, comments and order kept (store.rs); a secret goes to the Secret Service (secret.rs).
//!
//! The entries: ostrov's own sections, every control centre widget with a schema ([widget.ID]), and whatever
//! registers one (a plugin, its schema sent as JSON: Schema is Deserialize).

use std::cell::RefCell;
use std::rc::Rc;

use serde::Deserialize;
use serde_json::Value;

pub mod form;
mod secret;
mod store;

pub use secret::secret;

/// A form's sections.
#[derive(Clone, Deserialize, Default)]
pub struct Schema {
    pub sections: Vec<Section>,
}

#[derive(Clone, Deserialize)]
pub struct Section {
    /// the TOML table its fields live in ("calendar", "widget.wallpaper"); "" the entry's id
    #[serde(default)]
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub help: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Deserialize)]
pub struct Field {
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub help: String,
    #[serde(flatten)]
    pub kind: Kind,
    /// what it is while the file says nothing (null: nothing)
    #[serde(default)]
    pub default: Value,
    #[serde(default)]
    pub visible_if: Option<Cond>,
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Clone, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    String,
    Number {
        min: f64,
        max: f64,
        #[serde(default = "one")]
        step: f64,
        /// drawn as a slider rather than a spin button
        #[serde(default)]
        slider: bool,
    },
    Bool,
    Choice {
        options: Vec<Opt>,
    },
    Multi {
        options: Vec<Opt>,
    },
    /// a list of strings
    List,
    /// never in the file: in the Secret Service, as "<section>.<key>"
    Secret,
    Path,
    Color,
    /// seconds in the file, "1h30m" in the form
    Duration,
    Url,
}

fn one() -> f64 {
    1.0
}

/// An option: its value alone, or its value and what the form says for it.
#[derive(Clone, Deserialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum Opt {
    Plain(String),
    Labeled { value: String, label: String },
}

impl Opt {
    pub fn value(&self) -> &str {
        match self {
            Opt::Plain(v) | Opt::Labeled { value: v, .. } => v,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Opt::Plain(v) | Opt::Labeled { label: v, .. } => v,
        }
    }
}

/// Shown only while the section's key equals this.
#[derive(Clone, Deserialize)]
pub struct Cond {
    pub key: String,
    pub equals: Value,
}

/// What an action is handed: the field's value now, and how to say its outcome (any time later).
pub type Run = Rc<dyn Fn(&Value, Rc<dyn Fn(Result<String, String>)>)>;

/// A button under a field. From JSON it has an id and no run: the sender (a plugin's host) sets run before
/// registering the schema.
#[derive(Clone, Deserialize)]
pub struct Action {
    pub label: String,
    #[serde(default)]
    pub id: String,
    #[serde(skip)]
    pub run: Option<Run>,
}

impl Field {
    pub fn new(key: &str, title: &str, kind: Kind) -> Field {
        Field {
            key: key.into(),
            title: title.into(),
            help: String::new(),
            kind,
            default: Value::Null,
            visible_if: None,
            actions: Vec::new(),
        }
    }

    pub fn help(mut self, h: &str) -> Field {
        self.help = h.into();
        self
    }

    pub fn default(mut self, v: impl Into<Value>) -> Field {
        self.default = v.into();
        self
    }

    pub fn visible_if(mut self, key: &str, equals: impl Into<Value>) -> Field {
        self.visible_if = Some(Cond { key: key.into(), equals: equals.into() });
        self
    }

    pub fn action(mut self, label: &str, run: impl Fn(&Value, Rc<dyn Fn(Result<String, String>)>) + 'static) -> Field {
        self.actions.push(Action { label: label.into(), id: String::new(), run: Some(Rc::new(run)) });
        self
    }

    /// A number written as an integer: its range and step whole.
    pub fn integer(&self) -> bool {
        match self.kind {
            Kind::Number { min, max, step, .. } => min.fract() == 0.0 && max.fract() == 0.0 && step.fract() == 0.0,
            Kind::Duration => true,
            _ => false,
        }
    }
}

impl Section {
    pub fn new(key: &str, title: &str, fields: Vec<Field>) -> Section {
        Section { key: key.into(), title: title.into(), help: String::new(), fields }
    }

    pub fn help(mut self, h: &str) -> Section {
        self.help = h.into();
        self
    }
}

/// A page of the Settings list: an id (ostrov's section, "widget.ID", a plugin's), its row, its schema.
#[derive(Clone)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub schema: Schema,
}

thread_local! {
    /// Entries registered from elsewhere: the widgets', plugins'.
    static REGISTERED: RefCell<Vec<Entry>> = RefCell::default();
    /// The Settings page's list drawn again as an entry comes.
    static LISTENER: RefCell<Option<Box<dyn Fn()>>> = RefCell::default();
    /// The config's text the forms were last drawn from or wrote: a change that is not ours redraws them.
    static SEEN: RefCell<String> = RefCell::default();
    /// What follows such a change.
    static OUTSIDE: RefCell<Vec<Box<dyn Fn()>>> = RefCell::default();
}

/// An entry on the Settings page (replacing one of the same id): a widget's, a plugin's. Its sections' empty keys
/// are its id, so a plugin's fields go to [ID].
pub fn register(id: &str, title: &str, icon: &str, mut schema: Schema) {
    for s in schema.sections.iter_mut().filter(|s| s.key.is_empty()) {
        s.key = id.into();
    }
    REGISTERED.with(|r| {
        let mut r = r.borrow_mut();
        r.retain(|e| e.id != id);
        r.push(Entry { id: id.into(), title: title.into(), icon: icon.into(), schema });
    });
    LISTENER.with(|l| {
        if let Some(f) = l.borrow().as_ref() {
            f();
        }
    });
}

/// The entry of that id: a widget's (widget.ID), a plugin's (plugin.ID), one of ostrov's own sections.
pub fn entry(id: &str) -> Option<Entry> {
    entries().into_iter().find(|e| e.id == id)
}

/// ostrov's own entries, then the registered ones.
pub fn entries() -> Vec<Entry> {
    let mut all = own();
    all.extend(REGISTERED.with(|r| r.borrow().clone()));
    all
}

fn on_register(f: impl Fn() + 'static) {
    LISTENER.with(|l| *l.borrow_mut() = Some(Box::new(f)));
}

fn config_text() -> String {
    std::fs::read_to_string(crate::config::path()).unwrap_or_default()
}

/// f on every change to the config the forms did not write (an edit by hand, another program's).
pub fn on_outside(f: impl Fn() + 'static) {
    let first = OUTSIDE.with(|o| {
        let mut o = o.borrow_mut();
        o.push(Box::new(f));
        o.len() == 1
    });
    if first {
        crate::style::on_config(|| {
            if changed_outside() {
                OUTSIDE.with(|o| o.borrow().iter().for_each(|f| f()));
            }
        });
    }
}

/// Whether the config changed since the forms last saw it (an edit by hand), seen now if so.
fn changed_outside() -> bool {
    let now = config_text();
    SEEN.with(|s| {
        let mut s = s.borrow_mut();
        let changed = *s != now;
        *s = now;
        changed
    })
}

/// A field's value as the config has it, else its default.
pub fn value(table: &str, f: &Field) -> Value {
    let text = config_text();
    SEEN.with(|s| *s.borrow_mut() = text.clone());
    store::get(&text, table, &f.key).unwrap_or_else(|| f.default.clone())
}

/// The config with table's key set (None: taken out, its default again), written in place.
pub fn write(table: &str, key: &str, v: Option<&Value>, integer: bool) -> Result<(), String> {
    let path = crate::config::path();
    let text = config_text();
    let out = store::set(&text, table, key, v, integer)?;
    if out != text {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, &out).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    SEEN.with(|s| *s.borrow_mut() = out);
    Ok(())
}

/// A path's ~ the home.
pub fn expand(p: &str) -> std::path::PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => crate::hub::home().join(rest),
        None if p == "~" => crate::hub::home(),
        None => p.into(),
    }
}

/// What the form has for a field checked, as it goes into the file.
pub fn check(kind: &Kind, v: Value) -> Result<Value, String> {
    let text = v.as_str().unwrap_or("").trim().to_string();
    match kind {
        Kind::Number { min, max, .. } => {
            let n = v.as_f64().ok_or("not a number")?;
            if n < *min || n > *max {
                return Err(format!("between {min} and {max}"));
            }
        }
        Kind::Url if !text.is_empty() && !(text.starts_with("https://") || text.starts_with("http://")) => {
            return Err("an http:// or https:// address".into());
        }
        Kind::Color if !text.is_empty() && gtk4::gdk::RGBA::parse(text.as_str()).is_err() => {
            return Err("a colour: #5e81ac, rgb(94, 129, 172), a name".into());
        }
        Kind::Path if !text.is_empty() && !expand(&text).exists() => return Err("no such file or directory".into()),
        Kind::Duration => {
            let s = parse_duration(&text).ok_or("a duration: 90, 90s, 10m, 1h30m, 0 for never")?;
            return Ok(s.into());
        }
        Kind::Choice { options } if !options.iter().any(|o| o.value() == text) => {
            return Err("not one of the options".into());
        }
        Kind::List => {
            let items: Vec<Value> = v
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(str::trim).filter(|s| !s.is_empty()).map(Value::from))
                .collect();
            return Ok(items.into());
        }
        _ => {}
    }
    match v {
        Value::String(_) => Ok(text.into()),
        v => Ok(v),
    }
}

/// Seconds from "90", "90s", "10m", "1h30m", "1d".
pub fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Ok(n) = s.parse() {
        return Some(n);
    }
    let (mut total, mut num) = (0u64, String::new());
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let unit = match c {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            'd' => 86400,
            _ => return None,
        };
        total += num.parse::<u64>().ok()? * unit;
        num.clear();
    }
    (num.is_empty() && !s.is_empty()).then_some(total)
}

/// Seconds as parse_duration reads them back: 600 "10m", 5400 "1h30m".
pub fn fmt_duration(secs: u64) -> String {
    if secs == 0 {
        return "0".into();
    }
    let parts = [(secs / 86400, "d"), (secs / 3600 % 24, "h"), (secs / 60 % 60, "m"), (secs % 60, "s")];
    parts.iter().filter(|(n, _)| *n > 0).map(|(n, u)| format!("{n}{u}")).collect()
}

/// f run on a thread of its own (a secret, a test against a server), done handed its outcome on GTK's.
pub fn off_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let _ = tx.send_blocking(f());
    });
    gtk4::glib::spawn_future_local(async move {
        if let Ok(v) = rx.recv().await {
            done(v);
        }
    });
}

/// ostrov's own sections.
fn own() -> Vec<Entry> {
    let e = |id: &str, title: &str, icon: &str, sections| Entry {
        id: id.into(),
        title: title.into(),
        icon: icon.into(),
        schema: Schema { sections },
    };
    let blocks = "workspaces, window, record, privacy, layout, tray, panel.control (status), panel.calendar (clock), \
                  panel.ID";
    // the bar's defaults the ones config.rs takes, so the form shows what an empty file means
    let bar = crate::config::Bar::default();
    let options = |o: &[(&str, &str)]| -> Vec<Opt> {
        o.iter().map(|(v, l)| Opt::Labeled { value: v.to_string(), label: l.to_string() }).collect()
    };
    vec![
        e("appearance", "Appearance", "preferences-desktop-appearance-symbolic", appearance()),
        e("bar", "Bar", "view-continuous-symbolic", vec![Section::new("bar", "Bar", vec![
            Field::new("left", "Left", Kind::List).default(bar.left).help(blocks),
            Field::new("center", "Middle", Kind::List).default(bar.center),
            Field::new("right", "Right", Kind::List).default(bar.right),
        ])
        .help("The bar's blocks from left to right; taken at ostrov's start.")]),
        e("idle", "Idle", "preferences-system-time-symbolic", vec![Section::new("idle", "Idle", vec![
            Field::new("lock", "Lock after", Kind::Duration)
                .default(600)
                .help("Idle time to the lock screen; 0 never."),
            Field::new("screens_off", "Screens off after", Kind::Duration).default(900).help("0 never."),
        ])
        .help("Taken at ostrov's start.")]),
        e("calendar", "Calendar", "x-office-calendar-symbolic", vec![Section::new("calendar", "Calendar", vec![
            Field::new("caldav_url", "CalDAV server", Kind::Url)
                .help("https://caldav.icloud.com, Fastmail's, Nextcloud's…"),
            Field::new("user", "User", Kind::String),
            Field::new("password", "App password", Kind::Secret)
                .help("Kept in the keyring, never in the file.")
                .action("Test", |_, done| off_thread(crate::modules::calendar::service::test, move |r| done(r))),
            Field::new("password_command", "Password command", Kind::String)
                .help("Prints the password when the keyring has none: secret-tool lookup service caldav"),
            Field::new("ics", "Shared calendars", Kind::List).help(".ics and webcal:// links."),
        ])]),
        e("games", "Games", "input-gaming-symbolic", vec![Section::new("games", "Games", vec![
            Field::new("classes", "Window classes", Kind::List)
                .help("Their windows focused, the power profile below; a trailing * a prefix: dota2, cs2, steam_app_*."),
            Field::new("profile", "Power profile", Kind::Choice {
                options: options(&[
                    ("performance", "Performance"),
                    ("balanced", "Balanced"),
                    ("power-saver", "Power Saver"),
                ]),
            })
            .default("performance"),
        ])]),
        e("notifications", "Notifications", "preferences-system-notifications-symbolic", vec![Section::new(
            "notifications",
            "Notifications",
            vec![
                Field::new("quiet_in_games", "Quiet in games", Kind::Bool)
                    .default(true)
                    .help("Their toasts kept back while a game of [games] has the focus."),
                Field::new("quiet_from", "Quiet from", Kind::String).default("").help("A time of day, 23:00; empty: never."),
                Field::new("quiet_to", "Quiet to", Kind::String).default("").help("08:00"),
                Field::new("allow", "Let through", Kind::List).help("Apps whose toasts come all the same: Telegram Desktop."),
            ],
        )]),
        e("launcher", "Launcher", "system-search-symbolic", vec![Section::new("launcher", "Launcher", vec![
            Field::new("search", "Web search", Kind::Url)
                .default(crate::config::Launcher::default().search)
                .help("`s words` in the launcher; {} the words: https://www.google.com/search?q={}"),
        ])]),
    ]
}

/// [appearance]: the Appearance page draws theme and accent its own way, the rest as this form.
pub fn appearance() -> Vec<Section> {
    let opts = |o: &[&str]| o.iter().map(|v| Opt::Plain(v.to_string())).collect();
    let slider = |min, max, step| Kind::Number { min, max, step, slider: true };
    // the blur's as Hyprland has it now, while the file says nothing of it
    let live = |k: &str, or: i64| {
        let v = crate::wm::hyprctl(&format!("j/getoption decoration:blur:{k}"));
        serde_json::from_str::<Value>(&v).ok().and_then(|v| v["int"].as_i64()).unwrap_or(or)
    };
    // the theme's suggestions the defaults, what the file leaving them out gives
    let theme = crate::theme::get(&crate::config::load().appearance.theme);
    vec![Section::new("appearance", "Appearance", vec![
        Field::new("theme", "Theme", Kind::Choice {
            options: crate::theme::all().into_iter().map(|t| Opt::Labeled { value: t.id, label: t.name }).collect(),
        })
        .default("dark"),
        Field::new("accent", "Accent", Kind::Color).default("").help("Empty: the theme's own."),
        Field::new("surface", "Surface colour", Kind::Color)
            .default("")
            .help("Under everything that opens; empty: the theme's."),
        Field::new("opacity", "Surface opacity", slider(0.3, 1.0, 0.05)).default(0.75),
        Field::new("bar_color", "Bar colour", Kind::Color).default("").help("Empty: the theme's."),
        Field::new("bar_opacity", "Bar opacity", slider(0.0, 1.0, 0.05))
            .default(crate::modules::wallpaper::service::bar_alpha())
            .help("Unset, 0.65 over a wallpaper and solid over none."),
        Field::new("radius", "Corner radius", slider(0.0, 20.0, 1.0))
            .default(theme.radius.unwrap_or(10))
            .help("A surface's; what is on it 4 less."),
        Field::new("density", "Density", Kind::Choice { options: opts(&["compact", "normal", "comfortable"]) })
            .default(theme.density.unwrap_or("normal".into()))
            .help("The control centre's rows."),
        Field::new("animations", "Animations", Kind::Bool).default(true),
        Field::new("blur", "Blur", Kind::Bool)
            .default(live("enabled", 1) != 0)
            .help("Hyprland's, behind the bar and what opens."),
        Field::new("blur_size", "Blur size", slider(1.0, 20.0, 1.0))
            .default(live("size", 8))
            .visible_if("blur", true),
        Field::new("blur_passes", "Blur passes", slider(1.0, 4.0, 1.0))
            .default(live("passes", 1))
            .visible_if("blur", true),
        Field::new("font", "Font", Kind::String).default("").help("A family (Inter, Iosevka…); empty: GTK's own."),
        Field::new("font_size", "Text size", slider(8.0, 16.0, 1.0))
            .default(11.0)
            .help("In points; the rest sized from it."),
        Field::new("icon_size", "Icon size", slider(10.0, 32.0, 1.0)).default(16),
        Field::new("bar_icon_size", "Bar's icon size", slider(10.0, 32.0, 1.0)).default(14),
        Field::new("bar_padding", "Bar's block padding", slider(0.0, 24.0, 1.0)).default(10),
        Field::new("bar_spacing", "Bar's icon gap", slider(0.0, 24.0, 1.0)).default(7),
        Field::new("bar_height", "Bar's height", slider(18.0, 48.0, 1.0)).default(25),
    ])]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schema_from_json() {
        let s: Schema = serde_json::from_value(json!({"sections": [{"key": "plugin.weather", "title": "Weather",
            "fields": [
            {"key": "city", "title": "City", "type": "string", "default": "Moscow", "help": "Where"},
            {"key": "units", "title": "Units", "type": "choice", "options": ["metric", {"value": "us", "label": "US"}]},
            {"key": "every", "title": "Refresh", "type": "number", "min": 1, "max": 60},
            {"key": "token", "title": "Token", "type": "secret", "visible_if": {"key": "units", "equals": "us"},
             "actions": [{"id": "test", "label": "Test"}]},
            {"key": "wait", "title": "Wait", "type": "duration", "default": 600}
        ]}]}))
        .unwrap();
        let f = &s.sections[0].fields;
        assert_eq!(f[0].default, json!("Moscow"));
        let us = Opt::Labeled { value: "us".into(), label: "US".into() };
        assert_eq!(f[1].kind, Kind::Choice { options: vec![Opt::Plain("metric".into()), us] });
        assert_eq!(f[2].kind, Kind::Number { min: 1.0, max: 60.0, step: 1.0, slider: false });
        assert!(f[2].integer() && f[4].integer() && !f[0].integer());
        assert_eq!(f[3].kind, Kind::Secret);
        assert_eq!(f[3].visible_if.as_ref().map(|c| c.equals.clone()), Some(json!("us")));
        assert!(f[3].actions[0].id == "test" && f[3].actions[0].run.is_none());
        let bad = json!({"sections": [{"title": "x", "fields": [{"key": "a", "title": "A", "type": "nope"}]}]});
        assert!(serde_json::from_value::<Schema>(bad).is_err());
    }

    #[test]
    fn durations() {
        for (s, n) in [("90", 90), ("90s", 90), ("10m", 600), ("1h30m", 5400), ("1d", 86400), ("0", 0)] {
            assert_eq!(parse_duration(s), Some(n), "{s}");
        }
        assert_eq!(parse_duration("10x"), None);
        assert_eq!(parse_duration("1h30"), None);
        for n in [0, 45, 600, 5400, 90061] {
            assert_eq!(parse_duration(&fmt_duration(n)), Some(n));
        }
        assert_eq!(fmt_duration(5400), "1h30m");
    }

    #[test]
    fn checks() {
        let num = Kind::Number { min: 0.0, max: 20.0, step: 1.0, slider: true };
        assert_eq!(check(&num, json!(12.0)), Ok(json!(12.0)));
        assert!(check(&num, json!(21.0)).is_err());
        assert!(check(&Kind::Url, json!("caldav.example.com")).is_err());
        assert_eq!(check(&Kind::Url, json!(" https://x.org ")), Ok(json!("https://x.org")));
        assert_eq!(check(&Kind::Duration, json!("10m")), Ok(json!(600)));
        assert!(check(&Kind::Color, json!("#5e81ac")).is_ok() && check(&Kind::Color, json!("#zz")).is_err());
        assert_eq!(check(&Kind::List, json!(["a", " ", "b "])), Ok(json!(["a", "b"])));
    }
}
