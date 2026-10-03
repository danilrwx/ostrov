//! The desktop's state, from ostrov's services (services/, wmd's port), shared by the bar and its panels: every
//! new one replaces it and is handed to each part that asked, which redraws what it shows of it. The services'
//! commands, and the little helpers they all use.

use std::cell::RefCell;
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
    /// The hub, fed from the services for as long as the program runs.
    pub fn start() -> Rc<Hub> {
        let hub = Rc::new(Hub { state: RefCell::new(Value::Null), subs: RefCell::new(Vec::new()) });
        let (tx, rx) = async_channel::unbounded::<Value>();
        let (cmd_tx, cmd_rx) = async_channel::unbounded();
        crate::services::start(tx, cmd_rx);
        COMMANDS.with(|c| *c.borrow_mut() = Some(cmd_tx));
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

type Request = (Vec<String>, Option<String>, async_channel::Sender<crate::services::Res>);

thread_local! {
    /// The way to the services' commands, set as the hub starts.
    static COMMANDS: RefCell<Option<async_channel::Sender<Request>>> = const { RefCell::new(None) };
}

/// A service's command (wmd's words: wifi connect SSID, night mode sun), input what wmd read from its stdin, its
/// outcome (Ok, or what went wrong) handed to done on GTK's thread.
pub fn service_then(args: Vec<String>, input: Option<String>, done: impl Fn(crate::services::Res) + 'static) {
    let (tx, rx) = async_channel::bounded(1);
    let sent = COMMANDS.with(|c| c.borrow().as_ref().map(|c| c.send_blocking((args, input, tx)).is_ok()));
    if sent != Some(true) {
        return done(Err("no services".into()));
    }
    glib::spawn_future_local(async move {
        if let Ok(r) = rx.recv().await {
            done(r);
        }
    });
}

/// A service's command, its failure said on stderr.
pub fn service(args: &[&str]) {
    let what = args.join(" ");
    service_then(args.iter().map(|a| a.to_string()).collect(), None, move |r| {
        if let Err(e) = r {
            eprintln!("ostrov: {what}: {e}");
        }
    });
}

/// A value of the state as text, "" when missing.
pub fn s<'a>(v: &'a Value, path: &[&str]) -> &'a str {
    path.iter().fold(v, |v, k| &v[*k]).as_str().unwrap_or("")
}
