//! The overview, macOS's Mission Control: `ostrov overview` (a key, three fingers up) lays every window of every
//! workspace out in a grid over the blurred screen, each scaled down whole, its app's icon and title under it. A
//! click on one focuses it (its workspace with it), a click on nothing, Escape, `ostrov overview` again or `ostrov
//! overview close` (three fingers down) leave things as they were; the arrows, hjkl (Super with them or not),
//! Tab and Ctrl+N/P move the pick, Enter takes it. The layout is up at once with the apps' icons, the windows'
//! pictures filling in as Hyprland hands them over (hyprland-toplevel-export, on a Wayland connection of its own
//! off GTK's thread, as shot.rs takes the screen); windows on other workspaces too, which Hyprland renders for the
//! export.

use std::cell::{Cell, RefCell};
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;
use std::rc::Rc;

use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::{gdk, glib, Align, Orientation};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_registry::WlRegistry, wl_shm, wl_shm::WlShm, wl_shm_pool::WlShmPool};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols_hyprland::toplevel_export::v1::client::hyprland_toplevel_export_frame_v1::{
    self as export_frame, HyprlandToplevelExportFrameV1,
};
use wayland_protocols_hyprland::toplevel_export::v1::client::hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1;

use crate::style::label;

/// A rectangle: x, y, width, height.
type Rect = (f64, f64, f64, f64);

/// The space between the grid's windows, and under each for its icon and title.
const GAP: f64 = 24.0;
const FOOT: f64 = 32.0;
/// A thumbnail's longer side at most, in pixels: sharp still for a window shown near whole on a HiDPI screen.
const THUMB: u32 = 1600;

/// A window, as the overview shows it.
struct Win {
    address: String,
    class: String,
    title: String,
    workspace: i64,
    at: (f64, f64),
    size: (f64, f64),
    focused: bool,
}

/// Hyprland's windows (j/clients) on the workspaces past the special ones, top to bottom and left to right; the
/// unmapped and a group's hidden ones left out.
fn windows(clients: &str) -> Vec<Win> {
    let list: Value = serde_json::from_str(clients).unwrap_or_default();
    let pair = |v: &Value| (v[0].as_f64().unwrap_or_default(), v[1].as_f64().unwrap_or_default());
    let mut wins: Vec<Win> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["mapped"] != false && c["hidden"] != true && c["workspace"]["id"].as_i64().unwrap_or(0) > 0)
        .map(|c| Win {
            address: c["address"].as_str().unwrap_or_default().to_string(),
            class: c["class"].as_str().unwrap_or_default().to_string(),
            title: c["title"].as_str().unwrap_or_default().to_string(),
            workspace: c["workspace"]["id"].as_i64().unwrap_or_default(),
            at: pair(&c["at"]),
            size: pair(&c["size"]),
            focused: c["focusHistoryID"] == 0,
        })
        .collect();
    wins.sort_by(|a, b| (a.at.1, a.at.0).partial_cmp(&(b.at.1, b.at.0)).unwrap_or(std::cmp::Ordering::Equal));
    wins
}

/// The focused monitor: its connector's name, its logical place and size (x, y, w, h), its workspace.
fn monitor(monitors: &str) -> Option<(String, Rect, i64)> {
    let list: Value = serde_json::from_str(monitors).ok()?;
    let m = list.as_array()?.iter().find(|m| m["focused"] == true)?;
    let f = |k: &str| m[k].as_f64().unwrap_or_default();
    let scale = f("scale").max(0.1);
    let (mut w, mut h) = (f("width") / scale, f("height") / scale);
    // turned a quarter: its sides swapped
    if m["transform"].as_i64().unwrap_or(0) % 2 == 1 {
        std::mem::swap(&mut w, &mut h);
    }
    Some((m["name"].as_str()?.to_string(), (f("x"), f("y"), w, h), m["activeWorkspace"]["id"].as_i64()?))
}

/// Where windows of these sizes go in an area (w, h): a grid of as many columns as shows them largest, each
/// scaled into its cell keeping its shape (never past its own size), centred in it, FOOT-high room under it for
/// its title, gap between cells, the last row centred across. The pictures' rectangles, in the windows' order.
fn grid(sizes: &[(f64, f64)], (aw, ah): (f64, f64), gap: f64, foot: f64) -> Vec<Rect> {
    let n = sizes.len();
    let place = |cols: usize| -> Vec<Rect> {
        let rows = n.div_ceil(cols);
        let cw = (aw - gap * (cols - 1) as f64) / cols as f64;
        let ch = (ah - gap * (rows - 1) as f64) / rows as f64 - foot;
        sizes
            .iter()
            .enumerate()
            .map(|(i, &(w, h))| {
                let (row, col) = (i / cols, i % cols);
                let in_row = if row == rows - 1 { n - row * cols } else { cols };
                let left = (aw - in_row as f64 * cw - gap * (in_row - 1) as f64) / 2.0;
                let k = (cw / w.max(1.0)).min(ch / h.max(1.0)).clamp(0.0, 1.0);
                let (tw, th) = (w.max(1.0) * k, h.max(1.0) * k);
                (left + col as f64 * (cw + gap) + (cw - tw) / 2.0, row as f64 * (ch + foot + gap) + (ch - th) / 2.0, tw, th)
            })
            .collect()
    };
    let area = |rs: &Vec<Rect>| rs.iter().map(|r| r.2 * r.3).sum::<f64>();
    (1..=n).map(place).max_by(|a, b| area(a).total_cmp(&area(b))).unwrap_or_default()
}

/// The rectangle nearest from's way (dx, dy: a unit step, (1, 0) right), off to the side counting double; from
/// itself if there is none that way.
fn neighbor(rects: &[Rect], from: usize, (dx, dy): (f64, f64)) -> usize {
    let mid = |r: &Rect| (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
    let Some((fx, fy)) = rects.get(from).map(mid) else { return from };
    rects
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let (x, y) = mid(r);
            let along = (x - fx) * dx + (y - fy) * dy;
            let aside = ((x - fx) * dy).abs() + ((y - fy) * dx).abs();
            (along > 1.0).then_some((i, along + 2.0 * aside))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(from, |m| m.0)
}

/// A window's picture, shrunk to THUMB: its rows top first, 4 bytes a pixel.
struct Thumb {
    n: usize,
    width: u32,
    height: u32,
    format: gdk::MemoryFormat,
    data: Vec<u8>,
}

/// Pixels shrunk by a whole factor to max on their longer side at most, each the mean of its block; the rows top
/// first however they came (flipped: bottom first).
fn shrink(data: &[u8], (w, h, stride): (u32, u32, u32), flipped: bool, max: u32) -> (u32, u32, Vec<u8>) {
    let k = w.max(h).div_ceil(max).max(1);
    let (tw, th) = ((w / k).max(1), (h / k).max(1));
    let mut out = Vec::with_capacity((tw * th * 4) as usize);
    for ty in 0..th {
        for tx in 0..tw {
            let mut sum = [0u32; 4];
            let mut count = 0;
            for y in ty * k..((ty + 1) * k).min(h) {
                let row = if flipped { h - 1 - y } else { y };
                for x in tx * k..((tx + 1) * k).min(w) {
                    let at = (row * stride + x * 4) as usize;
                    if let Some(px) = data.get(at..at + 4) {
                        sum.iter_mut().zip(px).for_each(|(s, p)| *s += *p as u32);
                        count += 1;
                    }
                }
            }
            out.extend(sum.map(|s| (s / count.max(1)) as u8));
        }
    }
    (tw, th, out)
}

#[derive(Default)]
struct Take {
    shm: Option<(wl_shm::Format, u32, u32, u32)>,
    buffer_done: bool,
    flipped: bool,
    ready: bool,
    failed: bool,
}

impl Dispatch<WlRegistry, GlobalListContents> for Take {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlShm, ()> for Take {
    fn event(_: &mut Self, _: &WlShm, _: <WlShm as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlShmPool, ()> for Take {
    fn event(_: &mut Self, _: &WlShmPool, _: <WlShmPool as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlBuffer, ()> for Take {
    fn event(_: &mut Self, _: &WlBuffer, _: <WlBuffer as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<HyprlandToplevelExportManagerV1, ()> for Take {
    fn event(_: &mut Self, _: &HyprlandToplevelExportManagerV1, _: <HyprlandToplevelExportManagerV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<HyprlandToplevelExportFrameV1, ()> for Take {
    fn event(t: &mut Self, _: &HyprlandToplevelExportFrameV1, e: export_frame::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use export_frame::Event;
        match e {
            Event::Buffer { format: WEnum::Value(f), width, height, stride } => t.shm = Some((f, width, height, stride)),
            Event::BufferDone => t.buffer_done = true,
            Event::Flags { flags: WEnum::Value(f) } => t.flipped = f.contains(export_frame::Flags::YInvert),
            Event::Ready { .. } => t.ready = true,
            Event::Failed => t.failed = true,
            _ => {}
        }
    }
}

/// The windows' pictures (their index in the overview, their handle: the address's lower 32 bits), one by one,
/// to tx until it is closed.
fn capture(jobs: Vec<(usize, u32)>, tx: async_channel::Sender<Thumb>) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, mut queue) = registry_queue_init::<Take>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let manager = globals.bind::<HyprlandToplevelExportManagerV1, _, _>(&qh, 1..=2, ()).map_err(|_| "no toplevel export")?;
    let shm = globals.bind::<WlShm, _, _>(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
    for (n, handle) in jobs {
        // ignore_damage: a window that does not change would never be copied otherwise
        let frame = manager.capture_toplevel(0, handle, &qh, ());
        let thumb = copy(&mut queue, &shm, &frame);
        frame.destroy();
        match thumb {
            Ok((width, height, format, data)) => {
                if tx.send_blocking(Thumb { n, width, height, format, data }).is_err() {
                    break;
                }
            }
            // one window not given (gone since): the others still
            Err(e) => eprintln!("ostrov: overview: {e}"),
        }
    }
    Ok(())
}

/// One frame copied into shared memory of its size (a file in the runtime dir, unlinked once open), shrunk.
fn copy(queue: &mut EventQueue<Take>, shm: &WlShm, frame: &HyprlandToplevelExportFrameV1) -> Result<(u32, u32, gdk::MemoryFormat, Vec<u8>), String> {
    let mut t = Take::default();
    let qh = queue.handle();
    let mut wait = |t: &mut Take| -> Result<(), String> {
        queue.blocking_dispatch(t).map_err(|e| e.to_string())?;
        if t.failed { Err("Hyprland would not hand a window over".into()) } else { Ok(()) }
    };
    while t.shm.is_none() || !t.buffer_done {
        wait(&mut t)?;
    }
    let (format, width, height, stride) = t.shm.ok_or("no shm buffer")?;
    let gformat = match format {
        wl_shm::Format::Argb8888 => gdk::MemoryFormat::B8g8r8a8,
        wl_shm::Format::Xrgb8888 => gdk::MemoryFormat::B8g8r8x8,
        wl_shm::Format::Abgr8888 => gdk::MemoryFormat::R8g8b8a8,
        wl_shm::Format::Xbgr8888 => gdk::MemoryFormat::R8g8b8x8,
        f => return Err(format!("a pixel format not read here: {f:?}")),
    };
    let size = stride * height;
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let path = format!("{dir}/ostrov-overview-{}", std::process::id());
    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&path);
    file.set_len(size as u64).map_err(|e| e.to_string())?;
    let pool = shm.create_pool(file.as_fd(), size as i32, &qh, ());
    let buffer = pool.create_buffer(0, width as i32, height as i32, stride as i32, format, &qh, ());
    frame.copy(&buffer, 1);
    let mut done = Ok(());
    while !t.ready && done.is_ok() {
        done = wait(&mut t);
    }
    buffer.destroy();
    pool.destroy();
    done?;
    let mut data = vec![0; size as usize];
    file.read_exact_at(&mut data, 0).map_err(|e| e.to_string())?;
    let (w, h, rows) = shrink(&data, (width, height, stride), t.flipped, THUMB);
    Ok((w, h, gformat, rows))
}

/// A box of a size with a picture laid over it, the picture taking exactly that size whatever its own (GtkFixed
/// would give it its texture's).
fn frame(w: f64, h: f64, class: &str) -> (gtk4::Overlay, gtk4::Box, gtk4::Picture) {
    let under = gtk4::Box::new(Orientation::Vertical, 0);
    under.add_css_class(class);
    under.set_size_request(w.round() as i32, h.round() as i32);
    let pic = gtk4::Picture::new();
    pic.set_can_shrink(true);
    pic.set_content_fit(gtk4::ContentFit::Fill);
    let over = gtk4::Overlay::new();
    over.set_child(Some(&under));
    over.add_overlay(&pic);
    over.set_overflow(gtk4::Overflow::Hidden);
    (over, under, pic)
}

/// What is done with a click on a widget.
fn on_click(w: &impl IsA<gtk4::Widget>, f: impl Fn() + 'static) {
    let click = gtk4::GestureClick::new();
    click.connect_released(move |_, _, _, _| f());
    w.add_controller(click);
}

/// A dispatch to Hyprland once the overview is gone: Hyprland gives the keyboard back to the window it had as the
/// layer goes, which would undo a focus asked before; asked again once that is surely over.
fn after_close(cmd: String) {
    for ms in [30, 200] {
        let cmd = cmd.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || drop(crate::wm::hyprctl(&cmd)));
    }
}

pub struct Overview {
    win: gtk4::ApplicationWindow,
    body: gtk4::Box,
    /// the grid's windows: their cards, addresses, rectangles
    cards: RefCell<Vec<(gtk4::Widget, String, Rect)>>,
    picked: Cell<usize>,
    /// an opening's number: the pictures of one before it dropped as they come
    opening: Cell<u64>,
}

impl Overview {
    pub fn new(app: &gtk4::Application) -> Rc<Overview> {
        let win = gtk4::ApplicationWindow::new(app);
        win.init_layer_shell();
        win.set_layer(Layer::Overlay);
        win.set_namespace(Some("ostrov-overview"));
        for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            win.set_anchor(e, true);
        }
        win.set_exclusive_zone(-1);
        win.set_keyboard_mode(KeyboardMode::Exclusive);
        win.add_css_class("overview");
        let body = gtk4::Box::new(Orientation::Vertical, 0);
        body.add_css_class("overview-body");
        win.set_child(Some(&body));
        let o = Rc::new(Overview { win: win.clone(), body, cards: RefCell::default(), picked: Cell::new(0), opening: Cell::new(0) });

        // in the capture phase: Tab and the arrows would move GTK's focus first
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let me = Rc::downgrade(&o);
        keys.connect_key_pressed(move |_, k, _, m| {
            let Some(me) = me.upgrade() else { return glib::Propagation::Proceed };
            let ctrl = m.contains(gdk::ModifierType::CONTROL_MASK);
            match k {
                Key::n if ctrl => me.step(1),
                Key::p if ctrl => me.step(-1),
                Key::h => me.go_way((-1.0, 0.0)),
                Key::l => me.go_way((1.0, 0.0)),
                Key::k => me.go_way((0.0, -1.0)),
                Key::j => me.go_way((0.0, 1.0)),
                Key::Escape => me.close(),
                Key::Return | Key::KP_Enter => me.pick(me.picked.get()),
                Key::Tab => me.step(1),
                Key::ISO_Left_Tab => me.step(-1),
                Key::Left => me.go_way((-1.0, 0.0)),
                Key::Right => me.go_way((1.0, 0.0)),
                Key::Up => me.go_way((0.0, -1.0)),
                Key::Down => me.go_way((0.0, 1.0)),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        win.add_controller(keys);
        // a click on nothing: gone (a card's or a workspace's own click does its thing first)
        let me = Rc::downgrade(&o);
        on_click(&win, move || {
            if let Some(me) = me.upgrade() {
                me.close();
            }
        });
        o
    }

    pub fn toggle(self: &Rc<Self>) {
        if self.win.is_visible() { self.close() } else { self.open() }
    }

    /// Gone, the pointer left alone (moved, follow_mouse would focus the window under it), the pictures still
    /// coming dropped.
    pub fn close(&self) {
        self.opening.set(self.opening.get() + 1);
        self.win.set_visible(false);
    }

    fn open(self: &Rc<Self>) {
        let Some((name, (_, _, sw, sh), current)) = monitor(&crate::wm::hyprctl("j/monitors")) else { return };
        let wins = windows(&crate::wm::hyprctl("j/clients"));
        let on_screen = gdk::Display::default().map(|d| d.monitors()).and_then(|ms| {
            ms.iter::<gdk::Monitor>().flatten().find(|m| m.connector().as_deref() == Some(name.as_str()))
        });
        self.win.set_monitor(on_screen.as_ref());
        crate::style::clear(&self.body);
        // every window's pictures: its card's, its miniature's
        let mut pics: Vec<Vec<gtk4::Picture>> = vec![vec![]; wins.len()];

        let margin = 48.0;

        // every workspace's windows in a grid over the whole screen, the workspaces in their order, a
        // workspace's windows as they lie on it
        let top = margin;
        let mut mine: Vec<usize> = (0..wins.len()).collect();
        mine.sort_by_key(|&i| wins[i].workspace);
        let sizes: Vec<(f64, f64)> = mine.iter().map(|&i| wins[i].size).collect();
        let rects = grid(&sizes, (sw - 2.0 * margin, sh - top - margin), GAP, FOOT);
        let area = gtk4::Fixed::new();
        area.set_margin_top(margin as i32);
        area.set_margin_start(margin as i32);
        let mut cards = vec![];
        for (slot, (&i, &r)) in mine.iter().zip(&rects).enumerate() {
            let w = &wins[i];
            let icon = || match crate::switcher::icon(&w.class) {
                Some(i) => gtk4::Image::from_gicon(&i),
                None => gtk4::Image::from_icon_name("application-x-executable"),
            };
            let card = gtk4::Box::new(Orientation::Vertical, 6);
            card.add_css_class("ov-win");
            card.set_cursor_from_name(Some("pointer"));
            let (thumb, under, pic) = frame(r.2, r.3, "ov-thumb");
            thumb.add_css_class("ov-frame");
            let big = icon();
            big.set_pixel_size((r.2.min(r.3) / 3.0).clamp(16.0, 96.0) as i32);
            big.set_vexpand(true);
            under.append(&big);
            card.append(&thumb);
            let foot = gtk4::Box::new(Orientation::Horizontal, 6);
            foot.set_halign(Align::Center);
            let small = icon();
            small.set_pixel_size(16);
            foot.append(&small);
            let title = label(&w.title, "");
            title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            title.set_max_width_chars(((r.2 - 22.0) / 8.0).max(4.0) as i32);
            foot.append(&title);
            card.append(&foot);
            let me = Rc::downgrade(self);
            on_click(&card, move || {
                if let Some(me) = me.upgrade() {
                    me.pick(slot)
                }
            });
            // the card's padding round the picture
            area.put(&card, r.0 - 6.0, r.1 - 6.0);
            pics[i].push(pic);
            cards.push((card.upcast::<gtk4::Widget>(), w.address.clone(), r));
        }
        self.body.append(&area);
        let focused = mine.iter().position(|&i| wins[i].focused).unwrap_or(0);
        *self.cards.borrow_mut() = cards;
        self.picked.set(focused);
        self.set_picked(focused);

        // in with a short fade and swell: the body's class set once it has been drawn without it
        self.body.remove_css_class("shown");
        self.win.present();
        self.body.add_tick_callback(|b, _| {
            b.add_css_class("shown");
            glib::ControlFlow::Break
        });
        // clickable at once: Hyprland hands the pointer to a new surface only on a motion
        crate::wm::nudge();

        // the pictures, the grid's windows first
        let handle = |a: &str| u64::from_str_radix(a.trim_start_matches("0x"), 16).unwrap_or_default() as u32;
        let mut jobs: Vec<(usize, u32)> = wins.iter().enumerate().map(|(i, w)| (i, handle(&w.address))).collect();
        jobs.sort_by_key(|&(i, _)| wins[i].workspace != current);
        let (tx, rx) = async_channel::unbounded();
        std::thread::spawn(move || {
            if let Err(e) = capture(jobs, tx) {
                eprintln!("ostrov: overview: {e}");
            }
        });
        self.opening.set(self.opening.get() + 1);
        let (me, opening) = (Rc::downgrade(self), self.opening.get());
        glib::spawn_future_local(async move {
            while let Ok(t) = rx.recv().await {
                if me.upgrade().is_none_or(|me| me.opening.get() != opening) {
                    break;
                }
                let bytes = glib::Bytes::from_owned(t.data);
                let tex = gdk::MemoryTexture::new(t.width as i32, t.height as i32, t.format, &bytes, (t.width * 4) as usize);
                for p in pics.get(t.n).into_iter().flatten() {
                    p.set_paintable(Some(&tex));
                }
            }
        });
    }

    fn set_picked(&self, n: usize) {
        let cards = self.cards.borrow();
        if let Some(c) = cards.get(self.picked.get()) {
            c.0.remove_css_class("picked");
        }
        self.picked.set(n);
        if let Some(c) = cards.get(n) {
            c.0.add_css_class("picked");
        }
    }

    /// The pick moved by a step, round the ends.
    fn step(&self, by: isize) {
        let len = self.cards.borrow().len() as isize;
        if len > 0 {
            self.set_picked((self.picked.get() as isize + by).rem_euclid(len) as usize);
        }
    }

    /// The pick moved to the nearest window that way.
    fn go_way(&self, way: (f64, f64)) {
        let rects: Vec<Rect> = self.cards.borrow().iter().map(|c| c.2).collect();
        self.set_picked(neighbor(&rects, self.picked.get(), way));
    }

    /// The window focused (its workspace with it), once the overview is gone.
    fn pick(&self, n: usize) {
        let address = self.cards.borrow().get(n).map(|c| c.1.clone());
        self.close();
        if let Some(a) = address {
            after_close(format!("dispatch focuswindow address:{a}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(a: &Rect, b: &Rect) -> bool {
        a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
    }

    #[test]
    fn one_window_fills_the_area_keeping_its_shape() {
        assert_eq!(grid(&[(1000.0, 500.0)], (800.0, 600.0), 24.0, 0.0), [(0.0, 100.0, 800.0, 400.0)]);
        // never past its own size, centred
        assert_eq!(grid(&[(200.0, 100.0)], (800.0, 600.0), 24.0, 0.0), [(300.0, 250.0, 200.0, 100.0)]);
        assert!(grid(&[], (800.0, 600.0), 24.0, 32.0).is_empty());
    }

    #[test]
    fn four_screens_two_by_two() {
        let r = grid(&[(1600.0, 1000.0); 4], (1624.0, 1056.0), 24.0, 32.0);
        // cells 800 by 484 (516 less the foot): each 774.4 by 484
        assert_eq!(r.len(), 4);
        assert!((r[0].3 - 484.0).abs() < 1e-9 && (r[0].2 - 774.4).abs() < 1e-9);
        assert!(r[0].1 == r[1].1 && r[2].1 > r[0].1 + r[0].3 + 32.0);
        assert!(r[1].0 > r[0].0 + r[0].2 && (r[2].0 - r[0].0).abs() < 1e-9);
    }

    #[test]
    fn three_in_a_wide_area_a_row_the_last_row_centred() {
        let r = grid(&[(1000.0, 1000.0); 3], (3000.0, 1000.0), 0.0, 0.0);
        assert!(r.iter().all(|r| r.1 == 0.0 && r.3 == 1000.0));
        // five in a 3-2 grid: the bottom two centred under the three
        let r = grid(&[(100.0, 100.0); 5], (300.0, 200.0), 0.0, 0.0);
        assert_eq!((r[3].0, r[4].0), (50.0, 150.0));
    }

    #[test]
    fn any_windows_inside_and_apart() {
        let sizes = [(1560.0, 1015.0), (400.0, 900.0), (2000.0, 300.0), (800.0, 600.0), (777.0, 1333.0), (50.0, 50.0), (1200.0, 700.0)];
        for n in 1..=sizes.len() {
            let (aw, ah) = (1400.0, 700.0);
            let r = grid(&sizes[..n], (aw, ah), 24.0, 32.0);
            for (i, a) in r.iter().enumerate() {
                assert!(a.0 >= 0.0 && a.1 >= 0.0 && a.0 + a.2 <= aw + 1e-9 && a.1 + a.3 + 32.0 <= ah + 1e-9, "{n}: {a:?}");
                // the shape kept
                assert!((a.2 / a.3 - sizes[i].0 / sizes[i].1).abs() < 1e-9);
                // the foot under it free too
                let with_foot = (a.0, a.1, a.2, a.3 + 32.0);
                assert!(r.iter().enumerate().all(|(j, b)| i == j || !overlap(&with_foot, &(b.0, b.1, b.2, b.3 + 32.0))));
            }
        }
    }

    #[test]
    fn arrows_go_to_the_nearest_that_way() {
        // 0 1 2
        //  3 4
        let r = grid(&[(100.0, 100.0); 5], (300.0, 200.0), 0.0, 0.0);
        assert_eq!(neighbor(&r, 0, (1.0, 0.0)), 1);
        assert_eq!(neighbor(&r, 2, (1.0, 0.0)), 2);
        assert_eq!(neighbor(&r, 0, (0.0, 1.0)), 3);
        assert_eq!(neighbor(&r, 2, (0.0, 1.0)), 4);
        assert_eq!(neighbor(&r, 4, (0.0, -1.0)), 1);
        assert_eq!(neighbor(&r, 4, (-1.0, 0.0)), 3);
        assert_eq!(neighbor(&[], 0, (1.0, 0.0)), 0);
    }

    #[test]
    fn pictures_shrunk_by_their_blocks_mean() {
        // 2 by 2, a stride of 12: a pixel of padding a row
        let data = [0, 0, 0, 0, 4, 4, 4, 4, 9, 9, 9, 9, 8, 8, 8, 8, 12, 12, 12, 12, 9, 9, 9, 9];
        assert_eq!(shrink(&data, (2, 2, 12), false, 1), (1, 1, vec![6; 4]));
        assert_eq!(shrink(&data, (2, 2, 12), true, 2), (2, 2, [[8; 4], [12; 4], [0; 4], [4; 4]].concat()));
    }

    #[test]
    fn windows_by_place_specials_left_out() {
        let json = r#"[
            {"address":"0xa","class":"kitty","title":"a","workspace":{"id":2},"at":[800,30],"size":[700,900],"focusHistoryID":1},
            {"address":"0xb","class":"kitty","title":"b","workspace":{"id":2},"at":[0,30],"size":[700,900],"focusHistoryID":0},
            {"address":"0xc","class":"x","title":"c","workspace":{"id":-98},"at":[0,0],"size":[1,1],"focusHistoryID":2},
            {"address":"0xd","mapped":false,"class":"x","title":"d","workspace":{"id":1},"at":[0,0],"size":[1,1],"focusHistoryID":3}
        ]"#;
        let w = windows(json);
        assert_eq!(w.iter().map(|w| w.address.as_str()).collect::<Vec<_>>(), ["0xb", "0xa"]);
        assert!(w[0].focused && w[0].size == (700.0, 900.0) && w[1].at == (800.0, 30.0));
    }
}
