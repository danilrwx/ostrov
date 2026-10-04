//! ostrov's plugin protocol for plugins written in Rust: wit/ostrov-plugin.wit as JSON lines over stdin and stdout
//! (docs/plugins.md), the Python SDK's twin (sdk/python/ostrov_plugin.py). A plugin implements [`Plugin`], the
//! exports ostrov calls, each method left out answering as nothing; it calls ostrov's imports on the [`Host`] every
//! method is handed; [`run`] serves it until ostrov closes its stdin:
//!
//! ```no_run
//! use ostrov_plugin::{json, Host, Plugin, Value};
//!
//! struct Hello { on: bool }
//!
//! impl Plugin for Hello {
//!     fn render(&mut self, _: &Host, _widget: &str) -> Option<Value> {
//!         Some(json!({"type": "toggle", "id": "t", "title": "Hello", "on": self.on}))
//!     }
//!     fn on_event(&mut self, host: &Host, _widget: &str, _node: &str, _event: &str, value: &str) {
//!         self.on = value == "true";
//!         host.kick();
//!     }
//! }
//!
//! fn main() { ostrov_plugin::run(Hello { on: false }) }
//! ```
//!
//! The messages are read by a thread of their own, so the imports that answer ([`Host::ask`], [`Host::secret`],
//! [`Host::http_get`], [`Host::run`]) may be called from the plugin's methods and from any thread it spawns (a
//! [`Host`] is cheap to clone and Send): they block their caller until ostrov answers. A method blocked so holds
//! ostrov's other calls back (they wait 10 s), so a dialog, which waits for the user, is better asked from a
//! thread. A thread hands its outcome back to the plugin with `host.set_timer(0, ID)`, its [`Plugin::on_timer`]
//! called next on the plugin's own thread. Nothing but protocol may be printed on stdout: log with [`Host::log`]
//! or `eprintln!`.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
pub use serde_json::{self, Value, json};

/// The protocol's version this SDK speaks, the manifest's `api`.
pub const API: u32 = 1;

/// A message ostrov writes to the plugin: one of its exports called, or the answer to one of ostrov's imports it
/// called (`Return`). Unknown messages, a later ostrov's, read as `Unknown` and are passed over.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromOstrov {
    /// First, once per start: the protocol's version, and the user's language (two letters, "" for English).
    Hello {
        api: u32,
        #[serde(default)]
        language: String,
    },
    /// Its config, `[plugin.<id>]`: right after hello, and again on every change.
    OnConfig {
        #[serde(default)]
        json: Value,
    },
    State { call: u64 },
    /// `ostrov plugin <id> ARGS`, input what was piped to it: the one call answered by `RunResult`, by its id.
    RunRequest {
        id: u64,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        input: Option<String>,
    },
    /// A widget of the manifest drawn, `"<widget>#bar"` for its badge.
    Render { call: u64, widget: String },
    OnEvent { widget: String, node: String, event: String, value: String },
    OnTimer { id: u32 },
    /// The desktop's state (`ostrov dump`), with permission "state".
    OnState {
        #[serde(default)]
        json: Value,
    },
    /// A launcher mode, by its prefix, asked for its hits.
    Query { call: u64, mode: String, text: String },
    Pick { mode: String, id: String, text: String },
    /// One of ostrov's events the manifest follows, with permission "events".
    OnShellEvent {
        name: String,
        #[serde(default)]
        json: Value,
    },
    /// Its calendar's events between two local ISO times, with permission "calendar".
    CalendarEvents { call: u64, from: String, to: String },
    /// The answer to one of the plugin's calls, by its number: its value, or what went wrong.
    Return {
        call: u64,
        #[serde(default)]
        value: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// A message the plugin writes to ostrov: one of ostrov's imports called, a push, or the answer to an export.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToOstrov {
    Log { msg: String },
    /// One of ostrov's commands, with permission "run": answered `{"ok": out}` or `{"err": why}`.
    Run { call: u64, args: Vec<String> },
    /// A dialog, with permission "dialogs": answered with the answer, or null for a no.
    Ask { call: u64, dialog: Dialog },
    /// A secret field of its settings, with permission "secrets": answered with it, or null.
    Secret { call: u64, key: String },
    /// A URL's body, with permission "network": answered `{"ok": [bytes]}` or `{"err": why}`.
    HttpGet { call: u64, url: String },
    SetTimer { ms: u32, id: u32 },
    Kick,
    SetSettingsSchema { json: Value },
    /// A widget's tree pushed unasked, as render returning it.
    Render { widget: String, tree: Value },
    /// Its state pushed unasked, as state returning it.
    State { json: Value },
    /// The answer to ostrov's call by its number: the value, or `error` instead.
    Return {
        call: u64,
        #[serde(default)]
        value: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// The answer to a `RunRequest`, by its id: `ok` or `err`.
    RunResult {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ok: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        err: Option<String>,
    },
}

/// What a dialog is: a yes or no, a line of text, a password, one of options, a settings form.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DialogKind {
    #[default]
    Confirm,
    Text,
    Secret,
    Choice,
    Form,
}

/// A dialog's spec (docs/plugins.md, Dialogs): every field but `kind` optional, left out when empty. Written as
/// `Dialog { kind: DialogKind::Text, title: "Rename".into(), ..Default::default() }`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Dialog {
    pub kind: DialogKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// said in red under it: what went wrong last time
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    /// the buttons' words
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ok: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cancel: String,
    /// a text's entry: its placeholder and what it starts as
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub placeholder: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    /// a choice's options, a row each
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// a form's settings schema
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
}

/// A launcher mode's hit, a row of the launcher's: picked, ostrov opens `open`, else copies `copy`, else calls
/// [`Plugin::pick`] with its id.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Hit {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<String>,
}

/// An event of its calendar's. Times are local ISO 8601 without a zone ("2026-10-05T12:00:00"); a day's event
/// starts at its midnight and ends at the next, `all_day`. `end` empty is the start, `color` empty the theme's.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct CalendarEvent {
    pub title: String,
    pub start: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub end: String,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub location: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub color: String,
}

/// A plugin: the exports ostrov calls (docs/plugins.md, Exports), each on the plugin's own thread, one at a time,
/// in the order they come. Every method has a default answering as nothing, so a plugin writes those it needs.
pub trait Plugin {
    /// Its state as JSON, for `ostrov plugins`; called on every kick.
    fn state(&mut self, _host: &Host) -> Value {
        Value::Null
    }

    /// `ostrov plugin <id> ARGS` (and a settings field's action, `action ID` with the field's value as input),
    /// input what was piped to it: its output, or what went wrong (on stderr, exit status 1). Answered within 10 s.
    fn run(&mut self, _host: &Host, args: &[&str], _input: Option<&str>) -> Result<String, String> {
        Err(format!("no command {:?}", args.join(" ")))
    }

    /// A widget of its manifest drawn (`"<widget>#bar"` its badge): a tree of nodes, None to keep what is drawn.
    fn render(&mut self, _host: &Host, _widget: &str) -> Option<Value> {
        None
    }

    /// A node with an id used: click, toggle (value "true" or "false"), change (a slider's value, a chip's index,
    /// an entry's text).
    fn on_event(&mut self, _host: &Host, _widget: &str, _node: &str, _event: &str, _value: &str) {}

    /// A timer of [`Host::set_timer`] gone off.
    fn on_timer(&mut self, _host: &Host, _id: u32) {}

    /// Its config, `[plugin.<id>]`, at the start and on every change; [`Host::config`] has it afterwards too.
    fn on_config(&mut self, _host: &Host, _config: &Value) {}

    /// The desktop's state (`ostrov dump`), whenever it changes, with permission "state".
    fn on_state(&mut self, _host: &Host, _state: &Value) {}

    /// A launcher mode's hits (mode its prefix) for what is typed after the prefix; dropped after 2 s.
    fn query(&mut self, _host: &Host, _mode: &str, _text: &str) -> Vec<Hit> {
        vec![]
    }

    /// A hit without `open` or `copy` picked, text what was typed after the prefix.
    fn pick(&mut self, _host: &Host, _mode: &str, _id: &str, _text: &str) {}

    /// One of ostrov's events its manifest follows ("window" with {"class", "title"}, "power"...), with permission
    /// "events".
    fn on_shell_event(&mut self, _host: &Host, _name: &str, _payload: &Value) {}

    /// Its calendar's events between two local ISO times (manifest `calendar = true`, permission "calendar").
    fn calendar_events(&mut self, _host: &Host, _from: &str, _to: &str) -> Result<Vec<CalendarEvent>, String> {
        Ok(vec![])
    }
}

/// ostrov's imports (docs/plugins.md, Imports), and the plugin's config as last sent. Cloned and sent to other
/// threads freely: every clone writes to the same stdout, a line at a time.
#[derive(Clone)]
pub struct Host(Arc<Inner>);

struct Inner {
    out: Mutex<Box<dyn Write + Send>>,
    next: AtomicU64,
    calls: Mutex<Calls>,
    config: Mutex<Value>,
}

/// The plugin's calls waiting for their returns, by number. A return may be read before its caller waits for
/// it, so each side makes the slot it finds missing. Once ostrov is gone, the calls left fail.
#[derive(Default)]
struct Calls {
    gone: bool,
    slots: HashMap<u64, Slot>,
}

struct Slot {
    tx: Option<Sender<Result<Value, String>>>,
    rx: Option<Receiver<Result<Value, String>>>,
}

impl Calls {
    fn slot(&mut self, n: u64) -> &mut Slot {
        self.slots.entry(n).or_insert_with(|| {
            let (tx, rx) = channel();
            Slot { tx: Some(tx), rx: Some(rx) }
        })
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A WIT `result` as the JSON carries it, `{"ok": T}` or `{"err": why}`.
fn outcome<T: DeserializeOwned>(v: Result<Value, String>) -> Result<T, String> {
    let v = v?;
    if let Some(e) = v.get("err") {
        return Err(e.as_str().map_or_else(|| e.to_string(), String::from));
    }
    serde_json::from_value(v.get("ok").cloned().unwrap_or(Value::Null)).map_err(|e| format!("not a result: {e}"))
}

impl Host {
    fn new(out: impl Write + Send + 'static) -> Host {
        Host(Arc::new(Inner {
            out: Mutex::new(Box::new(out)),
            next: AtomicU64::new(1),
            calls: Mutex::default(),
            config: Mutex::new(json!({})),
        }))
    }

    /// A message written, a line of its own. A write that fails is ostrov gone, which the reader sees too.
    pub fn send(&self, m: &ToOstrov) {
        let Ok(mut line) = serde_json::to_vec(m) else { return };
        line.push(b'\n');
        let mut out = lock(&self.0.out);
        let _ = out.write_all(&line).and_then(|()| out.flush());
    }

    /// A call that answers, numbered, waited for; an error if ostrov failed it or is gone.
    fn call(&self, m: impl FnOnce(u64) -> ToOstrov) -> Result<Value, String> {
        let n = self.0.next.fetch_add(1, Ordering::Relaxed);
        let rx = {
            let mut calls = lock(&self.0.calls);
            if calls.gone && !calls.slots.contains_key(&n) {
                return Err("ostrov is gone".into());
            }
            calls.slot(n).rx.take()
        };
        self.send(&m(n));
        let r = rx.map_or(Err("ostrov is gone".into()), |rx| rx.recv().unwrap_or(Err("ostrov is gone".into())));
        lock(&self.0.calls).slots.remove(&n);
        r
    }

    /// A return read, handed to its caller.
    fn returned(&self, n: u64, r: Result<Value, String>) {
        if let Some(tx) = &lock(&self.0.calls).slot(n).tx {
            let _ = tx.send(r);
        }
    }

    /// ostrov gone: the calls waiting fail once what was answered is read.
    fn close(&self) {
        let mut calls = lock(&self.0.calls);
        calls.gone = true;
        calls.slots.values_mut().for_each(|s| s.tx = None);
    }

    fn answer(&self, call: u64, r: Result<Value, String>) {
        let (value, error) = match r {
            Ok(v) => (v, None),
            Err(e) => (Value::Null, Some(e)),
        };
        self.send(&ToOstrov::Return { call, value, error });
    }

    /// A line in ostrov's log, the plugin's id before it.
    pub fn log(&self, msg: impl std::fmt::Display) {
        self.send(&ToOstrov::Log { msg: msg.to_string() });
    }

    /// One of ostrov's commands, `ostrov ARGS` (permission "run"): its output, or what went wrong.
    pub fn run(&self, args: &[impl AsRef<str>]) -> Result<String, String> {
        let args = args.iter().map(|a| a.as_ref().to_string()).collect();
        outcome(self.call(|call| ToOstrov::Run { call, args }))
    }

    /// A toast of ostrov's own, `ostrov toast TITLE BODY` (permission "run").
    pub fn toast(&self, title: &str, body: &str) -> Result<String, String> {
        self.run(&["toast", title, body])
    }

    /// A dialog asked (permission "dialogs"), under the plugin's icon and id: the answer ("" for a confirm's yes,
    /// a form's a JSON object as text), None for a no, after 5 minutes, or while another of its dialogs is up.
    /// Waits for the user: never from [`Plugin::run`], better from a thread of its own.
    pub fn ask(&self, dialog: &Dialog) -> Option<String> {
        let dialog = dialog.clone();
        let v = self.call(|call| ToOstrov::Ask { call, dialog }).ok()?;
        v.as_str().map(String::from)
    }

    /// A secret field of its settings, as the Settings page keeps it in the keyring (permission "secrets").
    pub fn secret(&self, key: &str) -> Option<String> {
        let key = key.to_string();
        let v = self.call(|call| ToOstrov::Secret { call, key }).ok()?;
        v.as_str().map(String::from)
    }

    /// A URL's body (permission "network"), up to 16 MiB, in 20 s.
    pub fn http_get(&self, url: &str) -> Result<Vec<u8>, String> {
        let url = url.to_string();
        outcome(self.call(|call| ToOstrov::HttpGet { call, url }))
    }

    /// [`Plugin::on_timer`] called once with id, ms from now.
    pub fn set_timer(&self, ms: u32, id: u32) {
        self.send(&ToOstrov::SetTimer { ms, id });
    }

    /// Its state changed: ostrov calls state, and render for each of its widgets (and badges), again.
    pub fn kick(&self) {
        self.send(&ToOstrov::Kick);
    }

    /// Its settings' form on ostrov's Settings page: the Settings page's schema (sections of fields) as JSON.
    pub fn set_settings_schema(&self, schema: &Value) {
        self.send(&ToOstrov::SetSettingsSchema { json: schema.clone() });
    }

    /// A widget's tree pushed unasked, as if render had returned it.
    pub fn push_render(&self, widget: &str, tree: &Value) {
        self.send(&ToOstrov::Render { widget: widget.into(), tree: tree.clone() });
    }

    /// Its state pushed unasked, as if state had returned it.
    pub fn push_state(&self, state: &Value) {
        self.send(&ToOstrov::State { json: state.clone() });
    }

    /// Its config as last sent, `[plugin.<id>]` of ostrov's config.toml ({} before the first).
    pub fn config(&self) -> Value {
        lock(&self.0.config).clone()
    }

    /// One key of its config read as T, None if it is not there or not a T.
    pub fn setting<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        lock(&self.0.config).get(key).and_then(|v| T::deserialize(v).ok())
    }
}

/// The plugin served over stdin and stdout until ostrov closes its stdin.
pub fn run(plugin: impl Plugin) {
    serve(plugin, std::io::BufReader::new(std::io::stdin()), std::io::stdout());
}

/// The plugin served over any lines in and out, as [`run`] over stdin and stdout: a thread reads the lines, the
/// returns going to the calls waiting for them and the rest to the plugin, here, one at a time.
pub fn serve(mut plugin: impl Plugin, input: impl BufRead + Send + 'static, output: impl Write + Send + 'static) {
    let host = Host::new(output);
    let (tx, rx) = channel();
    let reader = host.clone();
    std::thread::spawn(move || {
        for line in input.lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str(&line) {
                Ok(FromOstrov::Return { call, value, error }) => reader.returned(call, error.map_or(Ok(value), Err)),
                Ok(m) => {
                    if tx.send(m).is_err() {
                        break;
                    }
                }
                Err(e) => eprintln!("ostrov-plugin: {e}: {line}"),
            }
        }
        reader.close();
    });
    for m in rx {
        handle(&mut plugin, &host, m);
    }
}

/// One of ostrov's messages handed to the plugin, its calls answered.
fn handle(p: &mut impl Plugin, h: &Host, m: FromOstrov) {
    let json = |v: Result<Value, serde_json::Error>| v.map_err(|e| e.to_string());
    match m {
        FromOstrov::Hello { api, language } => {
            if api != API {
                eprintln!("ostrov speaks api {api}, this plugin {API}");
            }
            let _ = WORDS.set(catalogue(Path::new("i18n"), &language));
            let _ = LANGUAGE.set(language);
        }
        FromOstrov::OnConfig { json } => {
            *lock(&h.0.config) = json.clone();
            p.on_config(h, &json);
        }
        FromOstrov::State { call } => {
            let v = p.state(h);
            h.answer(call, Ok(v));
        }
        FromOstrov::RunRequest { id, args, input } => {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let r = p.run(h, &args, input.as_deref());
            let (ok, err) = match r {
                Ok(o) => (Some(o), None),
                Err(e) => (None, Some(e)),
            };
            h.send(&ToOstrov::RunResult { id, ok, err });
        }
        FromOstrov::Render { call, widget } => {
            let v = p.render(h, &widget).unwrap_or(Value::Null);
            h.answer(call, Ok(v));
        }
        FromOstrov::OnEvent { widget, node, event, value } => p.on_event(h, &widget, &node, &event, &value),
        FromOstrov::OnTimer { id } => p.on_timer(h, id),
        FromOstrov::OnState { json } => p.on_state(h, &json),
        FromOstrov::Query { call, mode, text } => {
            let hits = p.query(h, &mode, &text);
            h.answer(call, json(serde_json::to_value(hits)));
        }
        FromOstrov::Pick { mode, id, text } => p.pick(h, &mode, &id, &text),
        FromOstrov::OnShellEvent { name, json } => p.on_shell_event(h, &name, &json),
        FromOstrov::CalendarEvents { call, from, to } => {
            let r = p.calendar_events(h, &from, &to);
            h.answer(call, r.and_then(|evs| json(serde_json::to_value(evs))));
        }
        FromOstrov::Return { .. } | FromOstrov::Unknown => {}
    }
}

static WORDS: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
static LANGUAGE: OnceLock<String> = OnceLock::new();

/// A language's catalogue of the plugin's texts, `<dir>/<lang>.toml` of `"English" = "theirs"` lines as ostrov's
/// own i18n/ are; none for English or a language without one.
fn catalogue(dir: &Path, lang: &str) -> HashMap<String, &'static str> {
    if lang.is_empty() || !lang.chars().all(|c| c.is_ascii_alphabetic()) {
        return HashMap::new();
    }
    let path = dir.join(format!("{lang}.toml"));
    let Ok(text) = std::fs::read_to_string(&path) else { return HashMap::new() };
    match toml::from_str::<HashMap<String, String>>(&text) {
        Ok(words) => words.into_iter().map(|(k, v)| (k, &*Box::leak(v.into_boxed_str()))).collect(),
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            HashMap::new()
        }
    }
}

/// The user's language as ostrov said it in hello (two letters, "" for English or before hello).
pub fn language() -> &'static str {
    LANGUAGE.get().map_or("", String::as_str)
}

/// A text of the plugin's in the user's language: looked up by its English in `i18n/<language>.toml` beside its
/// manifest (its working directory), read once at hello; the English itself where there is none.
pub fn t(en: &str) -> &str {
    WORDS.get().and_then(|w| w.get(en)).copied().unwrap_or(en)
}

/// The text's `{}` filled with the values, in order, so a language may move them: `fill(t("{} clicks"), &[&n])`.
pub fn fill(text: &str, values: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut values = values.iter();
    let mut rest = text;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(v) = values.next() {
            out.push_str(&v.to_string());
        }
        rest = &rest[i + 2..];
    }
    out + rest
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines of docs/plugins.md read and written back the same.
    #[test]
    fn exports_round_trip() {
        let lines = [
            r#"{"type":"hello","api":1,"language":"ru"}"#,
            r#"{"type":"on_config","json":{"greeting":"Privet"}}"#,
            r#"{"type":"state","call":3}"#,
            r#"{"type":"run_request","id":4,"args":["set","5"],"input":null}"#,
            r#"{"type":"render","call":5,"widget":"counter#bar"}"#,
            r#"{"type":"on_event","widget":"counter","node":"count","event":"toggle","value":"true"}"#,
            r#"{"type":"on_timer","id":7}"#,
            r#"{"type":"on_state","json":{"wifi":{"on":true}}}"#,
            r#"{"type":"query","call":8,"mode":"?","text":"why"}"#,
            r#"{"type":"pick","mode":"?","id":"1","text":"why"}"#,
            r#"{"type":"on_shell_event","name":"window","json":{"class":"foot","title":"~"}}"#,
            r#"{"type":"calendar_events","call":9,"from":"2026-09-24T00:00:00","to":"2026-11-08T00:00:00"}"#,
            r#"{"type":"return","call":1,"value":{"ok":"done"}}"#,
            r#"{"type":"return","call":2,"value":null,"error":"no answer in 10 s"}"#,
        ];
        for l in lines {
            let m: FromOstrov = serde_json::from_str(l).unwrap();
            assert_ne!(m, FromOstrov::Unknown, "{l}");
            let back: Value = serde_json::to_value(&m).unwrap();
            assert_eq!(back, serde_json::from_str::<Value>(l).unwrap(), "{l}");
        }
        // an older ostrov's hello, a later one's message
        let m: FromOstrov = serde_json::from_str(r#"{"type":"hello","api":1}"#).unwrap();
        assert_eq!(m, FromOstrov::Hello { api: 1, language: String::new() });
        let m: FromOstrov = serde_json::from_str(r#"{"type":"on_later","x":1}"#).unwrap();
        assert_eq!(m, FromOstrov::Unknown);
    }

    #[test]
    fn imports_round_trip() {
        let lines = [
            r#"{"type":"log","msg":"hi"}"#,
            r#"{"type":"run","call":1,"args":["menu","wifi"]}"#,
            r#"{"type":"ask","call":2,"dialog":{"kind":"choice","title":"Profile","options":["work","home"]}}"#,
            r#"{"type":"secret","call":3,"key":"token"}"#,
            r#"{"type":"http_get","call":4,"url":"https://example.org"}"#,
            r#"{"type":"set_timer","ms":1000,"id":7}"#,
            r#"{"type":"kick"}"#,
            r#"{"type":"set_settings_schema","json":{"sections":[]}}"#,
            r#"{"type":"render","widget":"counter","tree":{"type":"label","text":"1"}}"#,
            r#"{"type":"state","json":{"count":1}}"#,
            r#"{"type":"return","call":5,"value":[{"text":"Ask: why","open":"https://example.org/?q=why"}]}"#,
            r#"{"type":"run_result","id":4,"ok":"5"}"#,
            r#"{"type":"run_result","id":4,"err":"no"}"#,
        ];
        for l in lines {
            let m: ToOstrov = serde_json::from_str(l).unwrap();
            assert_eq!(serde_json::to_value(&m).unwrap(), serde_json::from_str::<Value>(l).unwrap(), "{l}");
        }
    }

    #[test]
    fn payloads() {
        let e: CalendarEvent = serde_json::from_value(json!({"title": "Lunch", "start": "2026-10-05T12:00:00",
            "end": "2026-10-05T13:00:00", "all_day": false, "location": "the kitchen", "color": "#e5a50a"}))
        .unwrap();
        assert_eq!((e.location.as_str(), e.all_day), ("the kitchen", false));
        let bare = CalendarEvent { title: "Day".into(), start: "2026-10-05T00:00:00".into(), ..Default::default() };
        assert_eq!(serde_json::to_value(bare).unwrap(), json!({"title": "Day", "start": "2026-10-05T00:00:00",
            "all_day": false}));
        let d = Dialog { kind: DialogKind::Text, title: "Rename".into(), value: "Hello".into(), ..Default::default() };
        assert_eq!(serde_json::to_value(d).unwrap(), json!({"kind": "text", "title": "Rename", "value": "Hello"}));
        assert_eq!(outcome::<Vec<u8>>(Ok(json!({"ok": [104, 105]}))), Ok(b"hi".to_vec()));
        assert_eq!(outcome::<String>(Ok(json!({"err": "not permitted: run"}))), Err("not permitted: run".into()));
        assert_eq!(outcome::<String>(Err("gone".into())), Err("gone".into()));
    }

    #[test]
    fn texts() {
        let dir = std::env::temp_dir().join(format!("ostrov-plugin-i18n-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ru.toml"), "# Russian\n\"{} clicks\" = \"{} нажатий\"\n").unwrap();
        let words = catalogue(&dir, "ru");
        assert_eq!(fill(words["{} clicks"], &[&5]), "5 нажатий");
        assert!(catalogue(&dir, "de").is_empty() && catalogue(&dir, "../ru").is_empty());
        assert_eq!(fill("{} of {}", &[&1]), "1 of ");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// What the plugin wrote, shared with the test.
    #[derive(Clone, Default)]
    struct Out(Arc<Mutex<Vec<u8>>>);

    impl Write for Out {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            lock(&self.0).extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct Counter {
        count: i64,
        asked: Option<String>,
    }

    impl Plugin for Counter {
        fn state(&mut self, _: &Host) -> Value {
            json!({"count": self.count})
        }
        fn run(&mut self, h: &Host, args: &[&str], input: Option<&str>) -> Result<String, String> {
            match args {
                ["set", n] => self.count = n.parse().map_err(|e| format!("{e}"))?,
                ["greeting"] => return Ok(h.setting::<String>("greeting").unwrap_or_default()),
                ["echo"] => return Ok(input.unwrap_or("").into()),
                _ => return Err(format!("no command {args:?}")),
            }
            h.kick();
            Ok(self.count.to_string())
        }
        fn render(&mut self, _: &Host, widget: &str) -> Option<Value> {
            (widget == "counter").then(|| json!({"type": "label", "text": self.count.to_string()}))
        }
        fn on_event(&mut self, _: &Host, _: &str, node: &str, _: &str, _: &str) {
            if node == "add" {
                self.count += 1;
            }
        }
        fn on_timer(&mut self, h: &Host, _: u32) {
            self.asked = h.ask(&Dialog { kind: DialogKind::Text, ..Default::default() });
            h.log(format!("asked {:?}", self.asked));
        }
        fn query(&mut self, _: &Host, _: &str, text: &str) -> Vec<Hit> {
            vec![Hit { text: format!("Ask: {text}"), copy: Some(text.into()), ..Default::default() }]
        }
        fn calendar_events(&mut self, _: &Host, _: &str, _: &str) -> Result<Vec<CalendarEvent>, String> {
            Err("offline".into())
        }
    }

    /// ostrov's lines in, the plugin's out: every call answered, a host call's return found by its number.
    #[test]
    fn serves() {
        let input = [
            r#"{"type":"hello","api":1,"language":""}"#,
            r#"{"type":"on_config","json":{"greeting":"Privet"}}"#,
            r#"{"type":"run_request","id":1,"args":["set","5"],"input":null}"#,
            r#"{"type":"on_event","widget":"counter","node":"add","event":"click","value":""}"#,
            r#"{"type":"state","call":2}"#,
            r#"{"type":"render","call":3,"widget":"counter"}"#,
            r#"{"type":"render","call":4,"widget":"other"}"#,
            r#"{"type":"run_request","id":5,"args":["greeting"],"input":null}"#,
            r#"{"type":"run_request","id":6,"args":["echo"],"input":"piped"}"#,
            r#"{"type":"run_request","id":7,"args":["nope"]}"#,
            "not json",
            r#"{"type":"on_later"}"#,
            r#"{"type":"query","call":8,"mode":"?","text":"why"}"#,
            r#"{"type":"calendar_events","call":9,"from":"a","to":"b"}"#,
            r#"{"type":"on_timer","id":1}"#,
            r#"{"type":"return","call":1,"value":"Renamed"}"#,
        ]
        .join("\n");
        let out = Out::default();
        serve(Counter::default(), std::io::Cursor::new(input.into_bytes()), out.clone());
        let text = String::from_utf8(lock(&out.0).clone()).unwrap();
        let got: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let want = [
            json!({"type": "kick"}),
            json!({"type": "run_result", "id": 1, "ok": "5"}),
            json!({"type": "return", "call": 2, "value": {"count": 6}}),
            json!({"type": "return", "call": 3, "value": {"type": "label", "text": "6"}}),
            json!({"type": "return", "call": 4, "value": null}),
            json!({"type": "run_result", "id": 5, "ok": "Privet"}),
            json!({"type": "run_result", "id": 6, "ok": "piped"}),
            json!({"type": "run_result", "id": 7, "err": "no command [\"nope\"]"}),
            json!({"type": "return", "call": 8, "value": [{"text": "Ask: why", "copy": "why"}]}),
            json!({"type": "return", "call": 9, "value": null, "error": "offline"}),
            json!({"type": "ask", "call": 1, "dialog": {"kind": "text"}}),
            json!({"type": "log", "msg": "asked Some(\"Renamed\")"}),
        ];
        assert_eq!(got, want);
    }

    /// A call made once ostrov is gone fails rather than waiting for ever.
    #[test]
    fn gone() {
        let h = Host::new(Out::default());
        h.close();
        assert_eq!(h.secret("token"), None);
        assert_eq!(h.run(&["toast", "x"]), Err("ostrov is gone".into()));
    }
}
