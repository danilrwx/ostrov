//! Screenshots, in place of grim and slurp: the screen taken as it is (wlr-screencopy, on a Wayland connection
//! of its own, into shared memory), then shown frozen over everything, dimmed, for a region to be dragged out
//! of it (a click alone takes it all, Escape none); the region put on the clipboard as a PNG, and so into its
//! history (clip.rs). ostrov screenshot: Print, $mod+Shift+S, the quick settings' button.

use std::cell::{Cell, RefCell};
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_output::WlOutput, wl_registry::WlRegistry, wl_shm, wl_shm::WlShm, wl_shm_pool::WlShmPool};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;

/// The screen as taken: its pixels as the compositor wrote them.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    stride: u32,
    pub format: gdk::MemoryFormat,
    flipped: bool,
    data: Vec<u8>,
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
impl Dispatch<WlOutput, ()> for Take {
    fn event(_: &mut Self, _: &WlOutput, _: <WlOutput as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwlrScreencopyManagerV1, ()> for Take {
    fn event(_: &mut Self, _: &ZwlrScreencopyManagerV1, _: <ZwlrScreencopyManagerV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwlrScreencopyFrameV1, ()> for Take {
    fn event(t: &mut Self, _: &ZwlrScreencopyFrameV1, e: zwlr_screencopy_frame_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use zwlr_screencopy_frame_v1::Event;
        match e {
            Event::Buffer { format: WEnum::Value(f), width, height, stride } => t.shm = Some((f, width, height, stride)),
            Event::BufferDone => t.buffer_done = true,
            Event::Flags { flags: WEnum::Value(f) } => t.flipped = f.contains(zwlr_screencopy_frame_v1::Flags::YInvert),
            Event::Ready { .. } => t.ready = true,
            Event::Failed => t.failed = true,
            _ => {}
        }
    }
}

/// The screen's copier: a Wayland connection of its own, off GTK's thread, and the shared memory the
/// compositor copies into, kept for the next frame of the same size (a recording's, record.rs).
pub struct Screen {
    queue: EventQueue<Take>,
    qh: QueueHandle<Take>,
    manager: ZwlrScreencopyManagerV1,
    shm: WlShm,
    output: WlOutput,
    buf: Option<Shm>,
}

/// The shared memory: a file in the runtime dir, unlinked once open, a buffer over it of the size the
/// compositor asked for.
struct Shm {
    spec: (wl_shm::Format, u32, u32, u32),
    file: std::fs::File,
    pool: WlShmPool,
    buffer: WlBuffer,
}

impl Screen {
    pub fn open() -> Result<Screen, String> {
        let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
        let (globals, queue) = registry_queue_init::<Take>(&conn).map_err(|e| e.to_string())?;
        let qh = queue.handle();
        let manager = globals.bind::<ZwlrScreencopyManagerV1, _, _>(&qh, 1..=3, ()).map_err(|_| "no wlr-screencopy")?;
        let shm = globals.bind::<WlShm, _, _>(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
        let output = globals.bind::<WlOutput, _, _>(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
        Ok(Screen { queue, qh, manager, shm, output, buf: None })
    }

    /// The first output taken: all of it, or a region of it (x, y, w, h in its logical coordinates, the frame
    /// in its pixels), with the pointer or without.
    pub fn frame(&mut self, region: Option<(i32, i32, i32, i32)>, pointer: bool) -> Result<Frame, String> {
        let qh = self.qh.clone();
        let frame = match region {
            Some((x, y, w, h)) => self.manager.capture_output_region(pointer as i32, &self.output, x, y, w, h, &qh, ()),
            None => self.manager.capture_output(pointer as i32, &self.output, &qh, ()),
        };
        let taken = self.copy(&frame);
        frame.destroy();
        taken
    }

    fn copy(&mut self, frame: &ZwlrScreencopyFrameV1) -> Result<Frame, String> {
        let mut t = Take::default();
        // the buffer's size and format told, every one of them (version 3 says when) before one is made
        while t.shm.is_none() || (self.manager.version() >= 3 && !t.buffer_done) {
            self.wait(&mut t)?;
        }
        let spec @ (format, width, height, stride) = t.shm.ok_or("no shm buffer")?;
        let gformat = match format {
            wl_shm::Format::Argb8888 => gdk::MemoryFormat::B8g8r8a8,
            wl_shm::Format::Xrgb8888 => gdk::MemoryFormat::B8g8r8x8,
            wl_shm::Format::Abgr8888 => gdk::MemoryFormat::R8g8b8a8,
            wl_shm::Format::Xbgr8888 => gdk::MemoryFormat::R8g8b8x8,
            f => return Err(format!("a pixel format not read here: {f:?}")),
        };
        let size = stride * height;
        if self.buf.as_ref().is_none_or(|b| b.spec != spec) {
            if let Some(old) = self.buf.take() {
                old.buffer.destroy();
                old.pool.destroy();
            }
            let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
            let path = format!("{dir}/ostrov-shot-{}", std::process::id());
            let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&path);
            file.set_len(size as u64).map_err(|e| e.to_string())?;
            let pool = self.shm.create_pool(file.as_fd(), size as i32, &self.qh, ());
            let buffer = pool.create_buffer(0, width as i32, height as i32, stride as i32, format, &self.qh, ());
            self.buf = Some(Shm { spec, file, pool, buffer });
        }
        frame.copy(&self.buf.as_ref().ok_or("no shm buffer")?.buffer);
        while !t.ready {
            self.wait(&mut t)?;
        }
        let buf = self.buf.as_ref().ok_or("no shm buffer")?;
        let mut data = vec![0; size as usize];
        buf.file.read_exact_at(&mut data, 0).map_err(|e| e.to_string())?;
        Ok(Frame { width, height, stride, format: gformat, flipped: t.flipped, data })
    }

    fn wait(&mut self, t: &mut Take) -> Result<(), String> {
        self.queue.blocking_dispatch(t).map_err(|e| e.to_string())?;
        if t.failed {
            return Err("the compositor would not copy the screen".into());
        }
        Ok(())
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        if let Some(b) = self.buf.take() {
            b.buffer.destroy();
            b.pool.destroy();
        }
    }
}

impl Frame {
    /// The region (x, y, w, h, in the frame's pixels): its rows, top first.
    pub fn crop(&self, (x, y, w, h): (u32, u32, u32, u32)) -> Option<Vec<u8>> {
        let mut rows = Vec::with_capacity((w * h * 4) as usize);
        for r in y..y + h {
            let src = if self.flipped { self.height.checked_sub(r + 1)? } else { r };
            let at = (src * self.stride + x * 4) as usize;
            rows.extend_from_slice(self.data.get(at..at + (w * 4) as usize)?);
        }
        Some(rows)
    }
}

/// What is done with a region picked: the frozen screen's frame, the region in its pixels (x, y, w, h), and
/// the frame's pixels to a logical one.
pub type Then = Rc<dyn Fn(&Frame, (u32, u32, u32, u32), f64)>;

/// The whole screen as a PNG into a file, no region asked (ostrov capture FILE: for scripts).
pub fn capture(path: String) {
    std::thread::spawn(move || {
        let r = Screen::open().and_then(|mut s| s.frame(None, false)).and_then(|f| {
            let rows = f.crop((0, 0, f.width, f.height)).ok_or("the frame came short")?;
            let tex = gdk::MemoryTexture::new(f.width as i32, f.height as i32, f.format, &glib::Bytes::from_owned(rows), (f.width * 4) as usize);
            tex.save_to_png(&path).map_err(|e| e.to_string())
        });
        if let Err(e) = r {
            eprintln!("ostrov: capture: {e}");
        }
        crate::hub::trim_heap();
    });
}

pub struct Shot {
    app: gtk4::Application,
    busy: Cell<bool>,
}

impl Shot {
    pub fn new(app: &gtk4::Application) -> Rc<Shot> {
        Rc::new(Shot { app: app.clone(), busy: Cell::new(false) })
    }

    /// The screen taken, a region asked for over it, put on the clipboard as a PNG.
    pub fn take(self: &Rc<Self>) {
        self.ask(Rc::new(|frame, region, _| {
            let Some(rows) = frame.crop(region) else { return };
            let (format, (_, _, w, h)) = (frame.format, region);
            // a PNG of the whole screen takes a while: made off GTK's thread
            let (tx, rx) = async_channel::bounded(1);
            std::thread::spawn(move || {
                let tex = gdk::MemoryTexture::new(w as i32, h as i32, format, &glib::Bytes::from_owned(rows), (w * 4) as usize);
                let _ = tx.send_blocking(tex.save_to_png_bytes().to_vec());
                drop(tex);
                crate::hub::trim_heap();
            });
            glib::spawn_future_local(async move {
                if let Ok(png) = rx.recv().await {
                    crate::clip::put(png, "png");
                }
            });
        }));
    }

    /// The screen taken, then a region asked for over it, handed to then (a screenshot's, a recording's).
    pub fn ask(self: &Rc<Self>, then: Then) {
        if self.busy.replace(true) {
            return;
        }
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(Screen::open().and_then(|mut s| s.frame(None, false)));
        });
        let me = self.clone();
        glib::spawn_future_local(async move {
            match rx.recv().await {
                Ok(Ok(frame)) => me.select(frame, then),
                Ok(Err(e)) => {
                    eprintln!("ostrov: screenshot: {e}");
                    me.busy.set(false);
                }
                Err(_) => me.busy.set(false),
            }
        });
    }

    /// The frozen screen over everything, a region dragged out of it.
    fn select(self: &Rc<Self>, frame: Frame, then: Then) {
        let tex = gdk::MemoryTexture::new(
            frame.width as i32,
            frame.height as i32,
            frame.format,
            &glib::Bytes::from(&frame.data),
            frame.stride as usize,
        );
        let win = gtk4::ApplicationWindow::new(&self.app);
        win.init_layer_shell();
        win.set_layer(Layer::Overlay);
        win.set_namespace(Some("ostrov-shot"));
        for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            win.set_anchor(e, true);
        }
        win.set_exclusive_zone(-1);
        win.set_keyboard_mode(KeyboardMode::Exclusive);
        win.add_css_class("shot");
        let over = gtk4::Overlay::new();
        let pic = gtk4::Picture::for_paintable(&tex);
        pic.set_content_fit(gtk4::ContentFit::Fill);
        over.set_child(Some(&pic));
        let area = gtk4::DrawingArea::new();
        area.set_cursor_from_name(Some("crosshair"));
        over.add_overlay(&area);
        win.set_child(Some(&over));

        // the region dragged, in the window's coordinates: start, then the pointer now
        let region: Rc<RefCell<Option<(f64, f64, f64, f64)>>> = Rc::default();
        let r = region.clone();
        area.set_draw_func(move |_, cr, w, h| {
            // the screen dimmed but for the region, the region outlined
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.45);
            cr.rectangle(0.0, 0.0, w as f64, h as f64);
            if let Some((x0, y0, x1, y1)) = *r.borrow() {
                let (x, y) = (x0.min(x1), y0.min(y1));
                let (rw, rh) = ((x1 - x0).abs(), (y1 - y0).abs());
                cr.rectangle(x, y, rw, rh);
                cr.set_fill_rule(gtk4::cairo::FillRule::EvenOdd);
                let _ = cr.fill();
                cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
                cr.set_line_width(1.0);
                cr.rectangle(x + 0.5, y + 0.5, rw - 1.0, rh - 1.0);
                let _ = cr.stroke();
            } else {
                let _ = cr.fill();
            }
        });

        // what was asked: None for nothing, Some(None) for the whole screen, else the region in the window's
        // coordinates, which go to the frame's pixels by its size over the window's
        let done = {
            let (me, win) = (self.clone(), win.clone());
            Rc::new(move |shot: Option<Option<(f64, f64, f64, f64)>>| {
                let k = frame.width as f64 / win.width().max(1) as f64;
                win.close();
                me.busy.set(false);
                // the frozen screen let go of once the window has gone
                glib::timeout_add_local_once(std::time::Duration::from_secs(1), crate::hub::trim_heap);
                let Some(region) = shot else { return };
                let (x, y, w, h) = region.unwrap_or((0.0, 0.0, frame.width as f64 / k, frame.height as f64 / k));
                let px = |v: f64| (v * k).round() as u32;
                let (x, y) = (px(x).min(frame.width - 1), px(y).min(frame.height - 1));
                let (w, h) = (px(w).clamp(1, frame.width - x), px(h).clamp(1, frame.height - y));
                then(&frame, (x, y, w, h), k);
            })
        };

        let drag = gtk4::GestureDrag::new();
        let (r, a) = (region.clone(), area.clone());
        drag.connect_drag_begin(move |_, x, y| {
            *r.borrow_mut() = Some((x, y, x, y));
            a.queue_draw();
        });
        let (r, a) = (region.clone(), area.clone());
        drag.connect_drag_update(move |g, dx, dy| {
            if let Some((x, y)) = g.start_point() {
                *r.borrow_mut() = Some((x, y, x + dx, y + dy));
                a.queue_draw();
            }
        });
        let (r, d) = (region.clone(), done.clone());
        drag.connect_drag_end(move |_, _, _| {
            let Some((x0, y0, x1, y1)) = *r.borrow() else { return };
            let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
            // a click alone, the whole screen
            d(Some((w >= 4.0 && h >= 4.0).then_some((x0.min(x1), y0.min(y1), w, h))));
        });
        area.add_controller(drag);

        let keys = gtk4::EventControllerKey::new();
        let d = done.clone();
        keys.connect_key_pressed(move |_, k, _, _| {
            if k == gdk::Key::Escape {
                d(None);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        win.add_controller(keys);
        win.present();
        // the pointer to the frozen screen at once: Hyprland hands it over only on a motion, the first click
        // lost without one
        crate::wm::nudge();
    }
}
