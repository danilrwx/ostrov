//! The desktop's state, from ostrov's services (services/, modules/), shared by the bar and its panels: every
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

    /// The state now.
    pub fn state(&self) -> Value {
        self.state.borrow().clone()
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

unsafe extern "C" {
    fn malloc_trim(pad: usize) -> i32;
}

/// The heap's free memory given back to the system. glibc keeps what a big buffer freed (a window's or a screen's
/// picture, tens of MB each), raising its threshold for big allocations as they go: done after such work, a
/// bar stays the size of a bar.
pub fn trim_heap() {
    // SAFETY: malloc_trim only walks glibc's own arenas
    unsafe { malloc_trim(0) };
}

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
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

/// A module's command (wifi connect SSID, power set balanced), input what it would read from its stdin, its
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
