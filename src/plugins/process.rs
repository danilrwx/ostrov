//! A plugin as a process: its exec run through sh in its directory, started again whenever it ends (1 s later,
//! twice as long each time it ends within a minute of starting, up to a minute), talked to in JSON lines over its
//! stdin and stdout, one message a line (docs/plugins.md). Every message is a call of wit/ostrov-plugin.wit: one
//! of the plugin's exports from ostrov, one of ostrov's imports from the plugin, a call that answers carrying a
//! "call" number its "return" carries back (run's request an "id" its result carries back), several at once. The
//! imports that need nothing of GTK's (log, kick, secret, http-get, set-timer) are answered here; run, ask,
//! set-settings-schema and the trees and state the plugin pushes go up to GTK's thread. What it writes on stderr
//! goes to ostrov's, its id before it. Its PATH has ostrov's own directory last, so an official plugin's binary,
//! installed beside ostrov, is found by its name (`exec = "ostrov-plugin-ID"`) wherever ostrov is, the user's
//! own programs still first.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{Draws, Up, API};
use crate::backend::{Backend, BoxFut, Kick};

pub struct Process(Arc<Inner>);

struct Inner {
    id: String,
    dir: PathBuf,
    exec: String,
    permissions: Vec<String>,
    config: Mutex<Value>,
    /// its stdin's lines, while it runs
    stdin: Mutex<Option<async_channel::Sender<String>>>,
    /// the calls made to it waiting for their return, by number
    calls: Mutex<HashMap<u64, async_channel::Sender<Result<Value, String>>>>,
    next: AtomicU64,
    up: async_channel::Sender<Up>,
    /// the plugins' Tokio runtime, once the worker runs on it: the calls' waits run there
    rt: Mutex<Option<tokio::runtime::Handle>>,
}

/// PATH with the directory of ostrov's own executable after the rest.
fn path() -> OsString {
    let own = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let all = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    std::env::join_paths(all.into_iter().chain(own)).unwrap_or_default()
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Process {
    pub fn new(m: &super::Manifest, dir: PathBuf, config: Value, up: async_channel::Sender<Up>) -> Process {
        Process(Arc::new(Inner {
            id: m.id.clone(),
            dir,
            exec: m.exec.clone(),
            permissions: m.permissions.clone(),
            config: Mutex::new(config),
            stdin: Mutex::default(),
            calls: Mutex::default(),
            next: AtomicU64::new(1),
            up,
            rt: Mutex::default(),
        }))
    }
}

impl Inner {
    fn allows(&self, what: &str) -> bool {
        self.permissions.iter().any(|p| p == what)
    }

    fn log(&self, what: &str) {
        eprintln!("ostrov: plugin {}: {what}", self.id);
    }

    fn send(&self, v: Value) {
        if let Some(tx) = lock(&self.stdin).as_ref() {
            let _ = tx.try_send(v.to_string());
        }
    }

    /// One of the plugin's exports called, its number in the message's field key: the answer, or why there is
    /// none. Waited for on the plugins' runtime, so it may be awaited on any thread.
    fn call(self: &Arc<Self>, v: Value, key: &'static str) -> BoxFut<Result<Value, String>> {
        let (tx, rx) = async_channel::bounded(1);
        match lock(&self.rt).as_ref() {
            Some(rt) => {
                let me = self.clone();
                rt.spawn(async move {
                    let _ = tx.send(me.ask(v, key).await).await;
                });
            }
            None => return Box::pin(std::future::ready(Err(format!("plugin {} not running", self.id)))),
        }
        Box::pin(async move { rx.recv().await.unwrap_or_else(|e| Err(e.to_string())) })
    }

    /// The call made: a plugin starting again waited for 2 s, its answer 10 s.
    async fn ask(&self, mut v: Value, key: &str) -> Result<Value, String> {
        for _ in 0..20 {
            if lock(&self.stdin).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if lock(&self.stdin).is_none() {
            return Err(format!("plugin {} not running", self.id));
        }
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        v[key] = json!(n);
        let (tx, rx) = async_channel::bounded(1);
        lock(&self.calls).insert(n, tx);
        self.send(v);
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err(format!("plugin {} ended", self.id)),
            Err(_) => {
                lock(&self.calls).remove(&n);
                Err(format!("plugin {}: no answer in 10 s", self.id))
            }
        }
    }

    /// A message from the plugin: a return, or one of ostrov's imports called, or a push.
    fn message(self: &Arc<Self>, v: Value, kick: &Kick) {
        let me = self.clone();
        let call = v["call"].clone();
        let answer = move |value: Value| me.send(json!({"type": "return", "call": call, "value": value}));
        let denied = |what: &str| {
            self.log(&format!("{}: not permitted: {what}", v["type"]));
            json!({"err": format!("not permitted: {what}")})
        };
        let text = |k: &str| v[k].as_str().unwrap_or("").to_string();
        match v["type"].as_str().unwrap_or("") {
            "return" => {
                let Some(tx) = v["call"].as_u64().and_then(|n| lock(&self.calls).remove(&n)) else { return };
                let r = match v["error"].as_str() {
                    Some(e) => Err(e.to_string()),
                    None => Ok(v["value"].clone()),
                };
                let _ = tx.try_send(r);
            }
            "run_result" => {
                let Some(tx) = v["id"].as_u64().and_then(|n| lock(&self.calls).remove(&n)) else { return };
                let _ = tx.try_send(Ok(v.clone()));
            }
            "log" => self.log(&text("msg")),
            "kick" => {
                let _ = kick.try_send(());
            }
            "run" if !self.allows("run") => answer(denied("run")),
            "run" => {
                let args = v["args"].as_array().into_iter().flatten();
                let args = args.filter_map(|a| a.as_str().map(String::from)).collect();
                let (tx, rx) = async_channel::bounded(1);
                let _ = self.up.try_send(Up::Run(args, tx));
                tokio::spawn(async move {
                    match rx.recv().await {
                        Ok(Ok(out)) => answer(json!({ "ok": out })),
                        Ok(Err(e)) => answer(json!({ "err": e })),
                        Err(_) => answer(json!({"err": "ostrov is going"})),
                    }
                });
            }
            "secret" if !self.allows("secrets") => {
                denied("secrets");
                answer(Value::Null);
            }
            // the Settings page's store: its secret fields, plugin.<id>.<key>
            "secret" => {
                let key = format!("plugin.{}.{}", self.id, text("key"));
                tokio::task::spawn_blocking(move || answer(json!(crate::settings::secret(&key))));
            }
            "ask" if !self.allows("dialogs") => {
                denied("dialogs");
                answer(Value::Null);
            }
            "ask" => {
                let (tx, rx) = async_channel::bounded(1);
                let _ = self.up.try_send(Up::Ask(v["dialog"].clone(), tx));
                tokio::spawn(async move { answer(json!(rx.recv().await.ok().flatten())) });
            }
            "http_get" if !self.allows("network") => answer(denied("network")),
            "http_get" => {
                let url = text("url");
                tokio::task::spawn_blocking(move || {
                    let agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(20))).build();
                    let body = agent.new_agent().get(&url).call();
                    let body = body.and_then(|mut r| r.body_mut().with_config().limit(16 << 20).read_to_vec());
                    answer(match body {
                        Ok(b) => json!({ "ok": b }),
                        Err(e) => json!({"err": e.to_string()}),
                    });
                });
            }
            "set_timer" => {
                let (me, ms, id) = (self.clone(), v["ms"].as_u64().unwrap_or(0), v["id"].clone());
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                    me.send(json!({"type": "on_timer", "id": id}));
                });
            }
            "set_settings_schema" => {
                let _ = self.up.try_send(Up::Settings(v["json"].clone()));
            }
            // the pushes: a widget's tree, the state, without being asked
            "render" => {
                let _ = self.up.try_send(Up::Render(text("widget"), v["tree"].clone()));
            }
            "state" => {
                let _ = self.up.try_send(Up::State(v["json"].clone()));
            }
            _ => {}
        }
    }
}

/// The worker stopped (the plugin removed, its task aborted): its stdin closed, so the plugin's own process ends
/// as its stdin does, not only sh (killed as the child is dropped); the calls waiting failed.
struct Stop(Arc<Inner>);

impl Drop for Stop {
    fn drop(&mut self) {
        *lock(&self.0.stdin) = None;
        lock(&self.0.calls).clear();
    }
}

impl Backend for Process {
    fn state(&self) -> BoxFut<Value> {
        let r = self.0.call(json!({"type": "state"}), "call");
        Box::pin(async move { r.await.unwrap_or(Value::Null) })
    }

    fn run(&self, args: &[String], input: Option<String>) -> BoxFut<Result<String, String>> {
        let r = self.0.call(json!({"type": "run_request", "args": args, "input": input}), "id");
        Box::pin(async move {
            let v = r.await?;
            match (v["ok"].as_str(), v["err"].as_str()) {
                (Some(out), _) => Ok(out.to_string()),
                (_, Some(e)) => Err(e.to_string()),
                _ => Err(format!("not a result: {v}")),
            }
        })
    }

    fn worker(&self, kick: Kick) -> BoxFut<()> {
        let me = self.0.clone();
        Box::pin(async move {
            let _stop = Stop(me.clone());
            *lock(&me.rt) = Some(tokio::runtime::Handle::current());
            let mut wait = Duration::from_secs(1);
            loop {
                let started = Instant::now();
                // exec: the shell gives its place to the plugin, not one more process waiting on it
                let child = tokio::process::Command::new("sh")
                    .args(["-c", &format!("exec {}", me.exec)])
                    .current_dir(&me.dir)
                    .env("PATH", path())
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true)
                    .spawn();
                let mut child = match child {
                    Ok(c) => c,
                    Err(e) => {
                        me.log(&format!("{}: {e}, again in {} s", me.exec, wait.as_secs()));
                        tokio::time::sleep(wait).await;
                        wait = (wait * 2).min(Duration::from_secs(60));
                        continue;
                    }
                };
                let (tx, rx) = async_channel::unbounded::<String>();
                if let Some(mut stdin) = child.stdin.take() {
                    tokio::spawn(async move {
                        while let Ok(line) = rx.recv().await {
                            if stdin.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                                break;
                            }
                        }
                    });
                }
                if let Some(err) = child.stderr.take() {
                    let id = me.id.clone();
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(err).lines();
                        while let Ok(Some(l)) = lines.next_line().await {
                            eprintln!("ostrov: plugin {id}: {l}");
                        }
                    });
                }
                *lock(&me.stdin) = Some(tx);
                me.send(json!({"type": "hello", "api": API, "language": crate::i18n::lang()}));
                let config = lock(&me.config).clone();
                me.send(json!({"type": "on_config", "json": config}));
                // drawn anew once it is up, as after any kick
                let _ = kick.try_send(());
                if let Some(out) = child.stdout.take() {
                    let mut lines = BufReader::new(out).lines();
                    while let Ok(Some(l)) = lines.next_line().await {
                        match serde_json::from_str(&l) {
                            Ok(v) => me.message(v, &kick),
                            Err(e) => me.log(&format!("{e}: {l}")),
                        }
                    }
                }
                *lock(&me.stdin) = None;
                lock(&me.calls).clear();
                let status = child.wait().await.map(|s| s.to_string()).unwrap_or_else(|e| e.to_string());
                if started.elapsed() > Duration::from_secs(60) {
                    wait = Duration::from_secs(1);
                }
                me.log(&format!("ended ({status}), again in {} s", wait.as_secs()));
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(Duration::from_secs(60));
            }
        })
    }
}

impl Draws for Process {
    fn render(&self, widget: &str) -> BoxFut<Result<Value, String>> {
        self.0.call(json!({"type": "render", "widget": widget}), "call")
    }

    fn on_event(&self, widget: &str, node: &str, event: &str, value: String) {
        self.0.send(json!({"type": "on_event", "widget": widget, "node": node, "event": event, "value": value}));
    }

    fn on_config(&self, config: Value) {
        *lock(&self.0.config) = config.clone();
        self.0.send(json!({"type": "on_config", "json": config}));
    }

    fn on_state(&self, state: &Value) {
        if self.0.allows("state") {
            self.0.send(json!({"type": "on_state", "json": state}));
        }
    }

    fn query(&self, mode: &str, text: &str) -> BoxFut<Result<Value, String>> {
        self.0.call(json!({"type": "query", "mode": mode, "text": text}), "call")
    }

    fn pick(&self, mode: &str, id: &str, text: &str) {
        self.0.send(json!({"type": "pick", "mode": mode, "id": id, "text": text}));
    }

    fn on_shell_event(&self, name: &str, payload: &Value) {
        self.0.send(json!({"type": "on_shell_event", "name": name, "json": payload}));
    }

    fn calendar_events(&self, from: &str, to: &str) -> BoxFut<Result<Value, String>> {
        self.0.call(json!({"type": "calendar_events", "from": from, "to": to}), "call")
    }
}
