//! ostrov's example plugin in Rust, hello-python's twin: a toggle counting its clicks by a step picked in its menu,
//! with a reset and a level; the window focused (one of ostrov's events) in its menu. What waits (a rename's
//! dialog, the keyring) is asked from threads of their own, so ostrov's calls go on being answered meanwhile: a
//! thread sends what it found over a channel and wakes the plugin with a timer, its on_timer taking it in.

use std::sync::mpsc::{Receiver, Sender, channel};

use ostrov_plugin::{Dialog, DialogKind, Host, Plugin, Value, fill, json, t};

const STEPS: [i64; 3] = [1, 5, 10];
/// The timer a thread wakes the plugin with, once it has sent what it found.
const WAKE: u32 = 1;

/// What a thread found.
enum Found {
    Title(String),
    Token(bool),
}

struct Hello {
    count: i64,
    on: bool,
    step: usize,
    level: f64,
    note: String,
    title: String,
    window: String,
    token: bool,
    found: (Sender<Found>, Receiver<Found>),
}

impl Hello {
    /// f run on a thread of its own, what it finds handed to the plugin.
    fn spawn(&self, host: &Host, f: impl FnOnce(&Host) -> Option<Found> + Send + 'static) {
        let (h, tx) = (host.clone(), self.found.0.clone());
        std::thread::spawn(move || {
            if let Some(found) = f(&h) {
                let _ = tx.send(found);
                h.set_timer(0, WAKE);
            }
        });
    }

    fn title(&self, host: &Host) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        host.setting("greeting").unwrap_or_else(|| t("Hello").to_string())
    }
}

impl Plugin for Hello {
    fn on_config(&mut self, host: &Host, _: &Value) {
        // ostrov's settings schema: on the Settings page as "Hello", the fields in [plugin.hello], the token in
        // the keyring
        host.set_settings_schema(&json!({"sections": [{"title": t("Hello"), "fields": [
            {"key": "greeting", "title": t("Greeting"), "type": "string", "default": t("Hello")},
            {"key": "token", "title": t("API token"), "type": "secret", "help": t("kept in the keyring"),
             "actions": [{"label": t("Test"), "id": "test-token"}]},
        ]}]}));
        // the keyring may ask to be unlocked first
        self.spawn(host, |h| Some(Found::Token(h.secret("token").is_some())));
        host.kick();
    }

    fn state(&mut self, _: &Host) -> Value {
        json!({"count": self.count, "on": self.on})
    }

    fn run(&mut self, host: &Host, args: &[&str], input: Option<&str>) -> Result<String, String> {
        match args {
            ["count"] => return Ok(self.count.to_string()),
            ["set", n] => self.count = n.parse().map_err(|e| format!("{n}: {e}"))?,
            ["toggle"] => self.on = !self.on,
            ["reset"] => self.count = 0,
            ["note"] => self.note = input.unwrap_or("").trim().to_string(),
            ["rename"] => {
                // a command answers within 10 s, the dialog waits for the user: asked from a thread
                let title = self.title(host);
                self.spawn(host, move |h| {
                    let d = Dialog {
                        kind: DialogKind::Text,
                        icon: "document-edit-symbolic".into(),
                        title: t("Rename the counter").into(),
                        text: t("Its toggle's title").into(),
                        value: title,
                        ok: t("Rename").into(),
                        ..Default::default()
                    };
                    h.ask(&d).filter(|s| !s.is_empty()).map(Found::Title)
                });
                return Ok("asking".into());
            }
            // the token's Test button, its value as input
            ["action", "test-token"] => {
                let n = input.unwrap_or("").chars().count();
                return match n {
                    0 => Err(t("no token typed").into()),
                    n => Ok(fill(t("a token of {} characters"), &[&n])),
                };
            }
            _ => return Err(format!("no command {:?}", args.join(" "))),
        }
        host.kick();
        Ok(self.count.to_string())
    }

    fn on_timer(&mut self, host: &Host, id: u32) {
        if id == WAKE {
            while let Ok(found) = self.found.1.try_recv() {
                match found {
                    Found::Title(t) => self.title = t,
                    Found::Token(on) => self.token = on,
                }
            }
            host.kick();
        }
    }

    fn on_shell_event(&mut self, host: &Host, name: &str, payload: &Value) {
        if name == "window" {
            self.window = payload["class"].as_str().unwrap_or("").to_string();
            host.kick();
        }
    }

    fn render(&mut self, host: &Host, widget: &str) -> Option<Value> {
        if widget == "counter#bar" {
            // its badge in the bar, shown while the toggle is on (the manifest's bar = "active")
            return Some(json!({"type": "box", "orientation": "horizontal", "active": self.on, "children": [
                {"type": "image", "icon": "face-smile-symbolic"},
                {"type": "label", "text": self.count.to_string()},
            ]}));
        }
        let note = if self.note.is_empty() { "echo text | ostrov plugin hello note" } else { &self.note };
        Some(json!({
            "type": "toggle", "id": "count", "icon": "face-smile-symbolic",
            "title": self.title(host), "sub": fill(t("{} clicks"), &[&self.count]), "on": self.on,
            "menu": {"type": "box", "children": [
                {"type": "row", "id": "add", "icon": "list-add-symbolic",
                 "text": fill(t("Add {}"), &[&STEPS[self.step]])},
                {"type": "row", "id": "reset", "icon": "edit-clear-symbolic", "text": t("Reset"),
                 "note": self.count.to_string()},
                {"type": "chips", "id": "step", "options": STEPS.map(|s| s.to_string()), "on": self.step},
                {"type": "slider", "id": "level", "icon": "weather-clear-symbolic", "value": self.level},
                {"type": "progress", "value": (self.count as f64 / 50.0).min(1.0)},
                {"type": "label", "text": note},
                {"type": "row", "icon": "window-new-symbolic",
                 "text": if self.window.is_empty() { t("no window") } else { &self.window }, "note": t("focused")},
                {"type": "label", "class": "dim",
                 "text": if self.token { t("a token is set") } else { t("no token: Settings, Hello") }},
            ]},
        }))
    }

    fn on_event(&mut self, host: &Host, _widget: &str, node: &str, _event: &str, value: &str) {
        match node {
            "count" => {
                self.on = value == "true";
                self.count += STEPS[self.step];
            }
            "add" => self.count += STEPS[self.step],
            "reset" => self.count = 0,
            "step" => self.step = value.parse().unwrap_or(0).min(STEPS.len() - 1),
            // the slider shows it already
            "level" => return self.level = value.parse().unwrap_or(self.level),
            _ => return,
        }
        host.kick();
    }
}

fn main() {
    ostrov_plugin::run(Hello {
        count: 0,
        on: false,
        step: 0,
        level: 0.5,
        note: String::new(),
        title: String::new(),
        window: String::new(),
        token: false,
        found: channel(),
    });
}
