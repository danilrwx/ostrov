//! The compositor ostrov runs under, Hyprland or sway, behind one face: the workspaces, going to one, the
//! events that change them, a popup's grab on the pointer. Hyprland through its sockets and its focus grab
//! protocol, sway through its IPC; under neither (another wlroots compositor) no workspaces and no grab.

use std::cell::RefCell;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gdk4_wayland::prelude::*;
use gtk4::prelude::*;
use gtk4::glib;
use serde_json::Value;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols_hyprland::focus_grab::v1::client::hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1;
use wayland_protocols_hyprland::focus_grab::v1::client::hyprland_focus_grab_v1::{self, HyprlandFocusGrabV1};

#[derive(Clone, Copy, PartialEq)]
pub enum Wm {
    Hyprland,
    Sway,
    Other,
}

pub fn wm() -> Wm {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        Wm::Hyprland
    } else if std::env::var_os("SWAYSOCK").is_some() {
        Wm::Sway
    } else {
        Wm::Other
    }
}

/// What the bar hears of the compositor.
pub enum Event {
    /// a workspace made, gone, focused, urgent: the dots to redraw
    Workspaces,
    /// a Super combination done (a workspace, a window): the compositor skips Super's release bind after another
    /// key, so a peek ends here instead
    Done,
    /// the window focused now, or its title changed: its class and title (empty: none)
    Window(String, String),
    /// the screen shared or recorded, or no longer: whether, and whether a window alone (Hyprland's screencast,
    /// said per client, so a share's end while another goes on reads as the end of all)
    Screencast(bool, bool),
}

pub(crate) fn hypr_socket(name: &str) -> Option<String> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let run = std::env::var("XDG_RUNTIME_DIR").ok()?;
    Some(format!("{run}/hypr/{sig}/{name}"))
}

/// A request to Hyprland's socket ("j/workspaces"), its answer.
pub(crate) fn hyprctl(req: &str) -> String {
    let Some(Ok(mut s)) = hypr_socket(".socket.sock").map(UnixStream::connect) else { return String::new() };
    let _ = s.write_all(req.as_bytes());
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out
}

/// A message to sway: i3's IPC, "i3-ipc", the payload's length and the type, then the payload.
fn sway_send(s: &mut UnixStream, kind: u32, payload: &str) -> std::io::Result<()> {
    let mut m = b"i3-ipc".to_vec();
    m.extend((payload.len() as u32).to_ne_bytes());
    m.extend(kind.to_ne_bytes());
    m.extend(payload.as_bytes());
    s.write_all(&m)
}

/// A message from sway: its type and payload.
fn sway_read(s: &mut UnixStream) -> std::io::Result<(u32, String)> {
    let mut head = [0u8; 14];
    s.read_exact(&mut head)?;
    let len = u32::from_ne_bytes(head[6..10].try_into().unwrap()) as usize;
    let kind = u32::from_ne_bytes(head[10..14].try_into().unwrap());
    let mut body = vec![0u8; len];
    s.read_exact(&mut body)?;
    Ok((kind, String::from_utf8_lossy(&body).into_owned()))
}

/// A request to sway (0 a command, 1 the workspaces), its answer.
fn swaymsg(kind: u32, payload: &str) -> String {
    let Some(Ok(mut s)) = std::env::var("SWAYSOCK").ok().map(UnixStream::connect) else { return String::new() };
    if sway_send(&mut s, kind, payload).is_err() {
        return String::new();
    }
    sway_read(&mut s).map(|r| r.1).unwrap_or_default()
}

/// The workspaces past the special ones, in order, and the focused one: those on monitor (its name, eDP-1) and the
/// one shown there, or with None all of them and the focused one.
pub fn workspaces(monitor: Option<&str>) -> (Vec<i64>, Option<i64>) {
    let (list, key, on) = match wm() {
        Wm::Hyprland => (hyprctl("j/workspaces"), "id", "monitor"),
        Wm::Sway => (swaymsg(1, ""), "num", "output"),
        Wm::Other => return (vec![], None),
    };
    let list: Value = serde_json::from_str(&list).unwrap_or_default();
    let ws = || list.as_array().into_iter().flatten().filter(|w| monitor.is_none_or(|m| w[on] == m));
    let mut ids: Vec<i64> = ws().filter_map(|w| w[key].as_i64()).filter(|i| *i > 0).collect();
    ids.sort();
    let focused = match (wm(), monitor) {
        (Wm::Hyprland, Some(m)) => {
            monitors().into_iter().find(|o| o["name"] == m).and_then(|o| o["activeWorkspace"]["id"].as_i64())
        }
        (Wm::Hyprland, None) => {
            serde_json::from_str::<Value>(&hyprctl("j/activeworkspace")).ok().and_then(|a| a["id"].as_i64())
        }
        (_, Some(_)) => ws().find(|w| w["visible"] == true).and_then(|w| w[key].as_i64()),
        _ => ws().find(|w| w["focused"] == true).and_then(|w| w[key].as_i64()),
    };
    (ids, focused)
}

/// The compositor's monitors (sway's outputs), as its IPC says them.
fn monitors() -> Vec<Value> {
    let list = match wm() {
        Wm::Hyprland => hyprctl("j/monitors"),
        Wm::Sway => swaymsg(3, ""),
        Wm::Other => return vec![],
    };
    serde_json::from_str::<Value>(&list).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default()
}

/// The monitor focused now, by its name (eDP-1); None under a compositor that does not say.
pub fn focused_monitor() -> Option<String> {
    monitors().into_iter().find(|o| o["focused"] == true).and_then(|o| o["name"].as_str().map(String::from))
}

pub fn go(id: i64) {
    match wm() {
        Wm::Hyprland => drop(hyprctl(&format!("dispatch workspace {id}"))),
        Wm::Sway => drop(swaymsg(0, &format!("workspace number {id}"))),
        Wm::Other => {}
    }
}

/// The compositor's events, for as long as it runs (on a thread of its own).
pub fn events(tx: async_channel::Sender<Event>) {
    match wm() {
        Wm::Hyprland => {
            let Some(Ok(s)) = hypr_socket(".socket2.sock").map(UnixStream::connect) else { return };
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                if let Some(e) = hypr_event(&line) {
                    let _ = tx.send_blocking(e);
                }
            }
        }
        Wm::Sway => {
            let Some(Ok(mut s)) = std::env::var("SWAYSOCK").ok().map(UnixStream::connect) else { return };
            if sway_send(&mut s, 2, r#"["workspace","window"]"#).is_err() || sway_read(&mut s).is_err() {
                return;
            }
            // workspace events (0x80000000) redraw and end a peek, window ones (0x80000003) end it
            while let Ok((kind, _)) = sway_read(&mut s) {
                if kind == 0x8000_0000 {
                    let _ = tx.send_blocking(Event::Workspaces);
                }
                let _ = tx.send_blocking(Event::Done);
            }
        }
        Wm::Other => {}
    }
}

/// A line of Hyprland's event socket ("name>>data") as an event, None for one that is not followed.
fn hypr_event(line: &str) -> Option<Event> {
    let (name, data) = line.split_once(">>")?;
    Some(match name {
        "activewindow" => {
            let (class, title) = data.split_once(',').unwrap_or((data, ""));
            Event::Window(class.into(), title.into())
        }
        "workspace" | "createworkspace" | "destroyworkspace" | "urgent" | "focusedmon" | "moveworkspace" => {
            Event::Workspaces
        }
        "workspacev2" | "focusedmonv2" | "activewindowv2" | "openwindow" | "closewindow" | "movewindowv2"
        | "changefloatingmode" | "fullscreen" => Event::Done,
        // STATE,OWNER: 1 or 0; 0 a monitor, 1 a window
        "screencast" => {
            let (state, owner) = data.split_once(',').unwrap_or((data, "0"));
            Event::Screencast(state == "1", owner == "1")
        }
        _ => return None,
    })
}

/// The screens on or off (idle.rs: off a while after the lock).
pub fn screens(on: bool) {
    match wm() {
        Wm::Hyprland => drop(hyprctl(if on { "dispatch dpms on" } else { "dispatch dpms off" })),
        Wm::Sway => drop(swaymsg(0, if on { "output * power on" } else { "output * power off" })),
        Wm::Other => {}
    }
}

/// The pointer put back where it is: Hyprland hands the pointer to a surface that maps or goes from under it only
/// on its next motion, so a click on the bar right after a popup closes would go nowhere.
pub fn nudge() {
    if wm() == Wm::Hyprland {
        glib::timeout_add_local_once(std::time::Duration::from_millis(30), || {
            crate::hub::run(&["sh", "-c", "hyprctl dispatch movecursor $(hyprctl cursorpos | tr -d ,)"]);
        });
    }
}

// Hyprland's focus grab: input kept to a set of surfaces (a popup, the bar), a click anywhere else clearing it,
// the pointer moving over a window without. On GTK's own Wayland connection, with a queue of ours read every
// 50 ms while a grab lasts.
struct Grabs;

impl Dispatch<WlRegistry, GlobalListContents> for Grabs {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<HyprlandFocusGrabManagerV1, ()> for Grabs {
    fn event(_: &mut Self, _: &HyprlandFocusGrabManagerV1, _: <HyprlandFocusGrabManagerV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<HyprlandFocusGrabV1, Arc<AtomicBool>> for Grabs {
    fn event(_: &mut Self, _: &HyprlandFocusGrabV1, e: hyprland_focus_grab_v1::Event, cleared: &Arc<AtomicBool>, _: &Connection, _: &QueueHandle<Self>) {
        if let hyprland_focus_grab_v1::Event::Cleared = e {
            cleared.store(true, Ordering::Relaxed);
        }
    }
}

struct GrabCtx {
    conn: Connection,
    queue: EventQueue<Grabs>,
    manager: HyprlandFocusGrabManagerV1,
}

thread_local! {
    /// None: not tried yet; Some(None): no focus grab here
    static GRAB: RefCell<Option<Option<GrabCtx>>> = const { RefCell::new(None) };
}

fn grab_ctx() -> Option<GrabCtx> {
    if wm() != Wm::Hyprland {
        return None;
    }
    let display = gtk4::gdk::Display::default()?.downcast::<gdk4_wayland::WaylandDisplay>().ok()?;
    let backend = display.wl_display()?.backend().upgrade()?;
    let conn = Connection::from_backend(backend);
    let (globals, queue) = registry_queue_init::<Grabs>(&conn).ok()?;
    let manager = globals.bind::<HyprlandFocusGrabManagerV1, _, _>(&queue.handle(), 1..=1, ()).ok()?;
    Some(GrabCtx { conn, queue, manager })
}

/// A grab while it lasts; dropped, it ends.
pub struct Grab {
    grab: HyprlandFocusGrabV1,
    timer: Option<glib::SourceId>,
}

impl Drop for Grab {
    fn drop(&mut self) {
        if let Some(t) = self.timer.take() {
            t.remove();
        }
        self.grab.destroy();
        GRAB.with(|g| {
            if let Some(Some(ctx)) = g.borrow().as_ref() {
                let _ = ctx.conn.flush();
            }
        });
    }
}

/// Input kept to these windows (mapped ones) until a click elsewhere, which calls cleared; None where there is
/// no focus grab (sway): the popup closes on losing the keyboard instead.
pub fn grab(windows: &[gtk4::Window], cleared: impl Fn() + 'static) -> Option<Grab> {
    GRAB.with(|g| {
        let mut g = g.borrow_mut();
        let ctx = g.get_or_insert_with(grab_ctx).as_mut()?;
        let flag = Arc::new(AtomicBool::new(false));
        let grab = ctx.manager.create_grab(&ctx.queue.handle(), flag.clone());
        for w in windows {
            if let Some(s) = w.surface().and_then(|s| s.downcast::<gdk4_wayland::WaylandSurface>().ok()).and_then(|s| s.wl_surface()) {
                grab.add_surface(&s);
            }
        }
        grab.commit();
        let _ = ctx.conn.flush();
        let timer = glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            GRAB.with(|g| {
                if let Some(Some(ctx)) = g.borrow_mut().as_mut() {
                    let _ = ctx.queue.dispatch_pending(&mut Grabs);
                }
            });
            // cleared: the popup closes, dropping the grab and this timer with it
            if flag.swap(false, Ordering::Relaxed) {
                cleared();
            }
            glib::ControlFlow::Continue
        });
        Some(Grab { grab, timer: Some(timer) })
    })
}

#[cfg(test)]
mod tests {
    use super::{hypr_event, Event};

    #[test]
    fn screencast() {
        assert!(matches!(hypr_event("screencast>>1,0"), Some(Event::Screencast(true, false))));
        assert!(matches!(hypr_event("screencast>>1,1"), Some(Event::Screencast(true, true))));
        assert!(matches!(hypr_event("screencast>>0,1"), Some(Event::Screencast(false, true))));
        let w = hypr_event("activewindow>>kitty,a,b");
        assert!(matches!(w, Some(Event::Window(c, t)) if c == "kitty" && t == "a,b"));
        assert!(hypr_event("submap>>").is_none());
    }
}
