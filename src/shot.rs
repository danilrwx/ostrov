//! Screenshots, in place of grim and slurp: the screen taken as it is (wlr-screencopy, on a Wayland connection
//! of its own, into shared memory), then shown frozen over everything, dimmed, for a region to be dragged out
//! of it (a click alone takes it all, Escape none); the region put on the clipboard as a PNG, and so into its
//! history (clip.rs). ostrov screenshot: Print, $mod+Shift+S, the quick settings' button.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::os::fd::AsFd;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_output::WlOutput, wl_registry::WlRegistry, wl_shm, wl_shm::WlShm, wl_shm_pool::WlShmPool};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1};
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;

/// The screen as taken: its pixels as the compositor wrote them.
struct Frame {
    width: u32,
    height: u32,
    stride: u32,
    format: gdk::MemoryFormat,
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

/// The screen (the first output) taken, off GTK's thread.
fn take() -> Result<Frame, String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, mut queue) = registry_queue_init::<Take>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let manager = globals.bind::<ZwlrScreencopyManagerV1, _, _>(&qh, 1..=3, ()).map_err(|_| "no wlr-screencopy")?;
    let shm = globals.bind::<WlShm, _, _>(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
    let output = globals.bind::<WlOutput, _, _>(&qh, 1..=1, ()).map_err(|e| e.to_string())?;
    let frame = manager.capture_output(0, &output, &qh, ());
    let mut t = Take::default();
    // the buffer's size and format told, every one of them (version 3 says when) before one is made
    while t.shm.is_none() || (manager.version() >= 3 && !t.buffer_done) {
        queue.blocking_dispatch(&mut t).map_err(|e| e.to_string())?;
        if t.failed {
            return Err("the compositor would not copy the screen".into());
        }
    }
    let (format, width, height, stride) = t.shm.ok_or("no shm buffer")?;
    let gformat = match format {
        wl_shm::Format::Argb8888 => gdk::MemoryFormat::B8g8r8a8,
        wl_shm::Format::Xrgb8888 => gdk::MemoryFormat::B8g8r8x8,
        wl_shm::Format::Abgr8888 => gdk::MemoryFormat::R8g8b8a8,
        wl_shm::Format::Xbgr8888 => gdk::MemoryFormat::R8g8b8x8,
        f => return Err(format!("a pixel format not read here: {f:?}")),
    };
    // the shared memory: a file in the runtime dir, unlinked once open
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let path = format!("{dir}/ostrov-shot-{}", std::process::id());
    let mut file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&path);
    let size = stride * height;
    file.set_len(size as u64).map_err(|e| e.to_string())?;
    let pool = shm.create_pool(file.as_fd(), size as i32, &qh, ());
    let buffer = pool.create_buffer(0, width as i32, height as i32, stride as i32, format, &qh, ());
    frame.copy(&buffer);
    while !t.ready {
        queue.blocking_dispatch(&mut t).map_err(|e| e.to_string())?;
        if t.failed {
            return Err("the compositor would not copy the screen".into());
        }
    }
    let mut data = Vec::with_capacity(size as usize);
    file.read_to_end(&mut data).map_err(|e| e.to_string())?;
    buffer.destroy();
    pool.destroy();
    frame.destroy();
    Ok(Frame { width, height, stride, format: gformat, flipped: t.flipped, data })
}

/// The region (x, y, w, h, in the frame's pixels) of a frame: its rows, top first.
fn crop(f: &Frame, (x, y, w, h): (u32, u32, u32, u32)) -> Option<Vec<u8>> {
    let mut rows = Vec::with_capacity((w * h * 4) as usize);
    for r in y..y + h {
        let src = if f.flipped { f.height - 1 - r } else { r };
        let at = (src * f.stride + x * 4) as usize;
        rows.extend_from_slice(f.data.get(at..at + (w * 4) as usize)?);
    }
    Some(rows)
}

pub struct Shot {
    app: gtk4::Application,
    busy: Cell<bool>,
}

impl Shot {
    pub fn new(app: &gtk4::Application) -> Rc<Shot> {
        Rc::new(Shot { app: app.clone(), busy: Cell::new(false) })
    }

    /// The screen taken, then a region asked for over it.
    pub fn take(self: &Rc<Self>) {
        if self.busy.replace(true) {
            return;
        }
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(take());
        });
        let me = self.clone();
        glib::spawn_future_local(async move {
            match rx.recv().await {
                Ok(Ok(frame)) => me.select(frame),
                Ok(Err(e)) => {
                    eprintln!("ostrov: screenshot: {e}");
                    me.busy.set(false);
                }
                Err(_) => me.busy.set(false),
            }
        });
    }

    /// The frozen screen over everything, a region dragged out of it.
    fn select(self: &Rc<Self>, frame: Frame) {
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
                let Some(region) = shot else { return };
                let (x, y, w, h) = region.unwrap_or((0.0, 0.0, frame.width as f64 / k, frame.height as f64 / k));
                let px = |v: f64| (v * k).round() as u32;
                let (x, y) = (px(x).min(frame.width - 1), px(y).min(frame.height - 1));
                let (w, h) = (px(w).clamp(1, frame.width - x), px(h).clamp(1, frame.height - y));
                let Some(rows) = crop(&frame, (x, y, w, h)) else { return };
                // a PNG of the whole screen takes a while: made off GTK's thread
                let format = frame.format;
                let (tx, rx) = async_channel::bounded(1);
                std::thread::spawn(move || {
                    let tex = gdk::MemoryTexture::new(w as i32, h as i32, format, &glib::Bytes::from_owned(rows), (w * 4) as usize);
                    let _ = tx.send_blocking(tex.save_to_png_bytes().to_vec());
                });
                glib::spawn_future_local(async move {
                    if let Ok(png) = rx.recv().await {
                        crate::clip::put(png, "png");
                    }
                });
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
