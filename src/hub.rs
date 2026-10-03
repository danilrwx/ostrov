//! The state wmd watch reports, shared by the bar and its panels: every new line replaces it and is handed to
//! each part that asked, which redraws what it shows of it. And the little helpers they all use.

use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

use gtk4::glib;
use serde_json::Value;

pub struct Hub {
    state: RefCell<Value>,
    subs: RefCell<Vec<Box<dyn Fn(&Value)>>>,
}

impl Hub {
    /// The hub, fed from wmd watch for as long as the program runs.
    pub fn start() -> Rc<Hub> {
        let hub = Rc::new(Hub { state: RefCell::new(Value::Null), subs: RefCell::new(Vec::new()) });
        let (tx, rx) = async_channel::unbounded::<Value>();
        std::thread::spawn(move || {
            loop {
                if let Ok(child) = Command::new(wmd()).arg("watch").stdout(Stdio::piped()).spawn() {
                    for line in BufReader::new(child.stdout.unwrap()).lines().map_while(Result::ok) {
                        if let Ok(v) = serde_json::from_str(&line) {
                            let _ = tx.send_blocking(v);
                        }
                    }
                }
                // back in a second should it ever exit
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });
        let h = hub.clone();
        glib::spawn_future_local(async move {
            while let Ok(v) = rx.recv().await {
                h.set(v);
            }
        });
        hub
    }

    /// f with the state now and on every change.
    pub fn on(&self, f: impl Fn(&Value) + 'static) {
        f(&self.state.borrow());
        self.subs.borrow_mut().push(Box::new(f));
    }

    fn set(&self, v: Value) {
        *self.state.borrow_mut() = v;
        let st = self.state.borrow().clone();
        for f in self.subs.borrow().iter() {
            f(&st);
        }
    }
}

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

pub fn wmd() -> PathBuf {
    home().join("go/bin/wmd")
}

/// A script of ~/dotfiles/bin.
pub fn bin(name: &str) -> String {
    home().join("dotfiles/bin").join(name).to_string_lossy().into_owned()
}

/// Run a command, detached, its exit reaped.
pub fn run(args: &[&str]) {
    let Some((cmd, rest)) = args.split_first() else { return };
    if let Ok(mut child) =
        Command::new(cmd).args(rest).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Run a command with input on its stdin, and hand its outcome (Ok, or its stderr) to done on GTK's thread.
pub fn run_then(args: Vec<String>, input: Option<String>, done: impl Fn(Result<(), String>) + 'static) {
    let (tx, rx) = async_channel::bounded::<Result<(), String>>(1);
    std::thread::spawn(move || {
        let child = Command::new(&args[0])
            .args(&args[1..])
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();
        let res = match child {
            Err(e) => Err(e.to_string()),
            Ok(mut c) => {
                if let (Some(i), Some(mut sin)) = (input, c.stdin.take()) {
                    use std::io::Write;
                    let _ = writeln!(sin, "{i}");
                }
                match c.wait_with_output() {
                    Ok(o) if o.status.success() => Ok(()),
                    Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().trim_start_matches("wmd: ").to_string()),
                    Err(e) => Err(e.to_string()),
                }
            }
        };
        let _ = tx.send_blocking(res);
    });
    glib::spawn_future_local(async move {
        if let Ok(r) = rx.recv().await {
            done(r);
        }
    });
}

/// A value of the state as text, "" when missing.
pub fn s<'a>(v: &'a Value, path: &[&str]) -> &'a str {
    path.iter().fold(v, |v, k| &v[*k]).as_str().unwrap_or("")
}
