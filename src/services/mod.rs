//! The desktop's state and switches, run on a Tokio runtime of their own (start), off GTK's thread: each module's
//! (modules/) state read, its commands run, its worker kept going. Here what they share: the buses, the state
//! put together and sent on as it changes, the commands handed to their module; and the helpers more than one
//! uses (D-Bus, rfkill, the location, the local time).
//!
//! The state is JSON, a key a module, what draws it reading its own; the commands are the modules' words
//! (wifi connect SSID, bt pair ADDR, power set balanced, ...).

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::modules::ALL;

pub mod dbus;
pub mod location;
pub mod rfkill;
pub mod time;

/// What every service reaches the desktop through: the system bus and the session bus.
pub struct Ctx {
    pub system: zbus::Connection,
    pub session: zbus::Connection,
}

/// A kick down to the watcher: something changed, the state is to be read again.
pub type Kick = async_channel::Sender<()>;

/// A command's outcome: Ok, or what went wrong, said to the user as it is.
pub type Res = Result<(), String>;

/// The modules' commands, a line each.
pub fn usage() -> String {
    ALL.iter().filter(|m| m.run.is_some()).map(|m| format!("  {} {}", m.id, m.forms.join(" | "))).collect::<Vec<_>>().join("\n")
}

/// The state now: each module's under its id.
pub async fn state(c: &Ctx) -> Value {
    let parts = ALL.iter().filter_map(|m| m.state.map(|f| async move { (m.id, f(c).await) }));
    Value::Object(futures_util::future::join_all(parts).await.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

/// A command, handed to the module it starts with; input is what it would read from its stdin (a
/// Wi-Fi passphrase).
pub async fn run(c: &Ctx, args: &[String], input: Option<String>) -> Res {
    let Some((first, rest)) = args.split_first() else { return Err(usage()) };
    match ALL.iter().find(|m| m.id == first).and_then(|m| m.run) {
        Some(f) => f(c, rest.to_vec(), input).await,
        None => Err(usage()),
    }
}

/// Whether ostrov ARGS is a module's command.
pub fn is_command(first: &str) -> bool {
    ALL.iter().any(|m| m.id == first && m.run.is_some())
}

/// The state, then again 100 ms after the last of a burst of kicks from the modules' workers (their services'
/// signals, their events); every 3 s besides, for what tells nothing (a network's strength, the backlight).
/// Only a state that differs from the last one sent goes out.
pub async fn watch(c: Arc<Ctx>, out: async_channel::Sender<Value>) {
    let (kick, kicks) = async_channel::unbounded::<()>();
    for m in ALL {
        if let Some(w) = m.worker {
            tokio::spawn(w(c.clone(), kick.clone()));
        }
    }

    let mut last = Value::Null;
    let mut emit = async |c: &Ctx| {
        let s = state(c).await;
        if s != last {
            last = s.clone();
            let _ = out.send(s).await;
        }
    };
    emit(&c).await;
    let mut tick = tokio::time::interval(Duration::from_secs(3));
    tick.tick().await;
    let settle = tokio::time::sleep(Duration::from_secs(3600));
    tokio::pin!(settle);
    let mut settling = false;
    loop {
        tokio::select! {
            Ok(()) = kicks.recv() => {
                settle.as_mut().reset(tokio::time::Instant::now() + Duration::from_millis(100));
                settling = true;
            }
            () = &mut settle, if settling => {
                settling = false;
                emit(&c).await;
            }
            _ = tick.tick() => emit(&c).await,
        }
    }
}

/// Every signal of a service's on the system bus (iwd's, BlueZ's...), as a kick.
pub async fn signals(c: Arc<Ctx>, sender: &str, kick: Kick) {
    use futures_util::StreamExt;
    let Ok(rule) = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).sender(sender).map(|b| b.build()) else {
        return;
    };
    let Ok(mut s) = zbus::MessageStream::for_match_rule(rule, &c.system, None).await else { return };
    while s.next().await.is_some() {
        let _ = kick.send(()).await;
    }
}

/// f every interval, a worker of what must be done now and then (the night light kept to its schedule).
pub async fn every<F: Future<Output = ()>>(interval: Duration, f: impl Fn() -> F) {
    let mut tick = tokio::time::interval(interval);
    loop {
        tick.tick().await;
        f().await;
    }
}

/// The services on a thread and Tokio runtime of their own: their state down state_tx as it changes, the
/// commands from cmds run as they come, each one's outcome back down its own channel.
pub fn start(state_tx: async_channel::Sender<Value>, cmds: async_channel::Receiver<(Vec<String>, Option<String>, async_channel::Sender<Res>)>) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() else { return };
        rt.block_on(async move {
            let (Ok(system), Ok(session)) = (zbus::Connection::system().await, zbus::Connection::session().await) else {
                eprintln!("ostrov: services: no D-Bus");
                return;
            };
            let c = Arc::new(Ctx { system, session });
            tokio::spawn(watch(c.clone(), state_tx));
            while let Ok((args, input, done)) = cmds.recv().await {
                let c = c.clone();
                tokio::spawn(async move {
                    let _ = done.send(run(&c, &args, input).await).await;
                });
            }
        });
    });
}
