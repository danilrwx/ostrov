//! Idle and sleep, in place of swayidle: the screen locked after 10 min without input and off after 15 (on
//! again at the next; the times config.rs's), locked before the machine sleeps and on logind's lock-session (loginctl lock-session, the
//! quick settings' button). Idle as the compositor tells it (ext-idle-notify, its inhibitors honoured: a video
//! playing keeps the screen on), on a Wayland connection and thread of its own; sleep and lock-session from
//! logind, a delay inhibitor held so the lock is up before the machine goes down.

use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{self, ExtIdleNotificationV1};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;

use crate::lock::Lock;

/// Kept awake (the quick settings' Keep Awake, ostrov awake): idle neither locks nor turns the screens off; the
/// lock before sleep and on lock-session stay.
static AWAKE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn awake() -> bool {
    AWAKE.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_awake(on: bool) {
    AWAKE.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// What the threads ask of GTK's.
enum Ask {
    Lock,
    /// the lock before sleep, told back once it is up
    SleepLock(async_channel::Sender<()>),
    Screens(bool),
}

/// Idle and sleep watched: the lock after times.lock seconds idle, the screens off after times.screens_off (0
/// for never, config.rs).
pub fn start(lock: &Rc<Lock>, times: &crate::config::Idle) {
    let (tx, rx) = async_channel::unbounded::<Ask>();
    let t = tx.clone();
    let (lock_after, off_after) = (times.lock, times.screens_off);
    std::thread::spawn(move || idle(t, lock_after, off_after));
    std::thread::spawn(move || logind(tx));
    let lock = lock.clone();
    glib::spawn_future_local(async move {
        while let Ok(a) = rx.recv().await {
            match a {
                Ask::Lock => lock.lock(),
                Ask::SleepLock(ack) => lock.lock_for_sleep(ack),
                Ask::Screens(on) => crate::wm::screens(on),
            }
        }
    });
}

/// Which notification an event is of: the lock's, or the screens'.
#[derive(Clone, Copy)]
enum Which {
    Lock,
    Off,
}

struct Idle(async_channel::Sender<Ask>);

impl Dispatch<WlRegistry, GlobalListContents> for Idle {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlSeat, ()> for Idle {
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ExtIdleNotifierV1, ()> for Idle {
    fn event(_: &mut Self, _: &ExtIdleNotifierV1, _: <ExtIdleNotifierV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ExtIdleNotificationV1, Which> for Idle {
    fn event(s: &mut Self, _: &ExtIdleNotificationV1, e: ext_idle_notification_v1::Event, w: &Which, _: &Connection, _: &QueueHandle<Self>) {
        let ask = match (e, w) {
            (ext_idle_notification_v1::Event::Idled, _) if awake() => return,
            (ext_idle_notification_v1::Event::Idled, Which::Lock) => Ask::Lock,
            (ext_idle_notification_v1::Event::Idled, Which::Off) => Ask::Screens(false),
            (ext_idle_notification_v1::Event::Resumed, Which::Off) => Ask::Screens(true),
            _ => return,
        };
        let _ = s.0.send_blocking(ask);
    }
}

/// The compositor's idle notifications, for as long as it runs.
fn idle(tx: async_channel::Sender<Ask>, lock_after: u32, off_after: u32) {
    let Ok(conn) = Connection::connect_to_env() else { return };
    let Ok((globals, mut queue)) = registry_queue_init::<Idle>(&conn) else { return };
    let qh = queue.handle();
    let (Ok(notifier), Ok(seat)) =
        (globals.bind::<ExtIdleNotifierV1, _, _>(&qh, 1..=1, ()), globals.bind::<WlSeat, _, _>(&qh, 1..=1, ()))
    else {
        eprintln!("ostrov: idle: no ext-idle-notify");
        return;
    };
    let _lock = (lock_after > 0).then(|| notifier.get_idle_notification(lock_after * 1000, &seat, &qh, Which::Lock));
    let _off = (off_after > 0).then(|| notifier.get_idle_notification(off_after * 1000, &seat, &qh, Which::Off));
    let mut state = Idle(tx);
    while queue.blocking_dispatch(&mut state).is_ok() {}
}

/// logind's word: the machine about to sleep (the lock first, the delay inhibitor let go once the compositor says
/// it is up, or after 3 s whatever it says, taken again on waking), the session asked to lock.
fn logind(tx: async_channel::Sender<Ask>) {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
    rt.block_on(async move {
        use futures_util::StreamExt;
        let Ok(conn) = zbus::Connection::system().await else { return };
        let inhibit = || async {
            let body = ("sleep", "ostrov", "the screen locked before sleep", "delay");
            let reply = conn
                .call_method(Some("org.freedesktop.login1"), "/org/freedesktop/login1", Some("org.freedesktop.login1.Manager"), "Inhibit", &body)
                .await
                .ok()?;
            reply.body().deserialize::<zbus::zvariant::OwnedFd>().ok()
        };
        let mut held = inhibit().await;
        // the session's own path: logind signals on it, not on the "auto" alias
        let session: String = match conn
            .call_method(Some("org.freedesktop.login1"), "/org/freedesktop/login1", Some("org.freedesktop.login1.Manager"), "GetSession", &("auto",))
            .await
        {
            Ok(r) => r.body().deserialize::<zbus::zvariant::OwnedObjectPath>().map(|p| p.to_string()).unwrap_or_default(),
            Err(_) => String::new(),
        };
        let rule = |member: &'static str, path: String| {
            zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender("org.freedesktop.login1")
                .and_then(|b| b.path(path))
                .and_then(|b| b.member(member))
                .map(|b| b.build())
        };
        let (Ok(sleep), Ok(lock)) = (rule("PrepareForSleep", "/org/freedesktop/login1".into()), rule("Lock", session))
        else {
            return;
        };
        let (Ok(sleep), Ok(lock)) = (
            zbus::MessageStream::for_match_rule(sleep, &conn, None).await,
            zbus::MessageStream::for_match_rule(lock, &conn, None).await,
        ) else {
            return;
        };
        let mut all = futures_util::stream::select(sleep.map(|m| (true, m)), lock.map(|m| (false, m)));
        while let Some((is_sleep, Ok(m))) = all.next().await {
            if !is_sleep {
                let _ = tx.send(Ask::Lock).await;
                continue;
            }
            if m.body().deserialize::<bool>().unwrap_or(false) {
                // the lock up before the inhibitor goes and the machine with it; a lock that never says so keeps
                // the machine up no longer than 3 s
                let (ack, up) = async_channel::bounded(1);
                let _ = tx.send(Ask::SleepLock(ack)).await;
                let _ = tokio::time::timeout(Duration::from_secs(3), up.recv()).await;
                held = None;
            } else if held.is_none() {
                held = inhibit().await;
            }
        }
        drop(held);
    });
}
