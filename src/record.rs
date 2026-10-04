//! Screen recording: `ostrov record [--audio]` asks for a region as a screenshot does (shot.rs's selector, a click
//! alone the whole screen), then records it to ~/Videos/Recordings/DATE_TIME.mp4 until `ostrov record` again or
//! a click on the bar's "record" block, a red dot and the time gone by, there only while recording. The frames
//! come through wlr-screencopy on a thread of their own (shot.rs's Screen, the region only), piped raw into
//! gst-launch-1.0 to be encoded (VA-API's H.264, else openh264's); --audio adds what the default sink plays.
//! Done, the file's path is on the clipboard and said in a notification.

use std::cell::{OnceCell, RefCell};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};

use crate::bar::{pill, slot, Block};
use crate::i18n::{fill, t};
use crate::shot::{Screen, Shot};

/// The video's frames a second.
const FPS: u32 = 30;

#[derive(Default)]
struct Rec {
    app: OnceCell<gtk4::Application>,
    shot: OnceCell<Rc<Shot>>,
    /// while recording: since when, and the recorder's thread's stop
    on: RefCell<Option<(Instant, Arc<AtomicBool>)>>,
}

thread_local! {
    static REC: Rc<Rec> = Rc::default();
}

/// What a recording asks the region with, and notifies with.
pub fn init(app: &gtk4::Application, shot: &Rc<Shot>) {
    REC.with(|r| {
        let _ = r.app.set(app.clone());
        let _ = r.shot.set(shot.clone());
    });
}

/// A recording begun (a region asked for first), or the one going stopped.
pub fn toggle(audio: bool) {
    let r = REC.with(Rc::clone);
    if let Some((_, stop)) = r.on.borrow().as_ref() {
        stop.store(true, Ordering::Relaxed);
        return;
    }
    let Some(shot) = r.shot.get() else { return };
    // the region in the output's logical coordinates, what screencopy takes a region in
    shot.ask(Rc::new(move |_, (x, y, w, h), k| {
        let l = |v: u32| (v as f64 / k).round() as i32;
        start((l(x), l(y), l(w).max(1), l(h).max(1)), audio);
    }));
}

fn start(region: (i32, i32, i32, i32), audio: bool) {
    let r = REC.with(Rc::clone);
    let stamp = glib::DateTime::now_local().ok().and_then(|t| t.format("%Y-%m-%d_%H-%M-%S").ok());
    let dir = crate::hub::home().join("Videos/Recordings");
    let file = dir.join(format!("{}.mp4", stamp.as_deref().unwrap_or("recording")));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = async_channel::bounded(1);
    let (s, f) = (stop.clone(), file.clone());
    std::thread::spawn(move || {
        let done = std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| record(region, &f, audio, &s));
        let _ = tx.send_blocking(done);
    });
    *r.on.borrow_mut() = Some((Instant::now(), stop));
    glib::spawn_future_local(async move {
        let Ok(done) = rx.recv().await else { return };
        r.on.borrow_mut().take();
        let (title, body) = match done {
            Ok(()) => {
                crate::clip::put(file.to_string_lossy().into_owned().into_bytes(), "text");
                (t("Recording saved"), file.display().to_string())
            }
            Err(e) => {
                eprintln!("ostrov: record: {e}");
                (t("Recording failed"), e)
            }
        };
        // GApplication's notification, to the notification server: ostrov's own (notes.rs)
        if let Some(app) = r.app.get() {
            let n = gio::Notification::new(title);
            n.set_body(Some(&body));
            app.send_notification(Some("ostrov-record"), &n);
        }
    });
}

/// The recorder, on a thread of its own: the region's frames into the encoder until stop, the file finished.
fn record(region: (i32, i32, i32, i32), file: &Path, audio: bool, stop: &AtomicBool) -> Result<(), String> {
    let mut screen = Screen::open()?;
    // the selector's frozen screen gone before the first frame
    std::thread::sleep(Duration::from_millis(200));
    let first = screen.frame(Some(region), true)?;
    let (w, h) = even(first.width, first.height).ok_or(t("a region too small to record"))?;
    let format = match first.format {
        gdk::MemoryFormat::B8g8r8a8 | gdk::MemoryFormat::B8g8r8x8 => "bgrx",
        _ => "rgbx",
    };
    let has = |e: &str| Command::new("gst-inspect-1.0").args(["--exists", e]).status().is_ok_and(|s| s.success());
    let encoder = ["vah264enc", "openh264enc"].into_iter().find(|e| has(e)).ok_or(t("no H.264 encoder in GStreamer"))?;
    let mut gst = Command::new("gst-launch-1.0")
        .args(pipeline(w, h, format, encoder, file, audio))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("gst-launch-1.0: {e}"))?;
    let mut pipe = gst.stdin.take().ok_or("no pipe to gst-launch-1.0")?;
    let fed = feed(&mut screen, first, region, (w, h), &mut pipe, stop);
    // the end of the frames the video's end; the audio, live, ends on gst-launch's -e at an interrupt
    drop(pipe);
    if audio {
        let _ = Command::new("kill").args(["-INT", &gst.id().to_string()]).status();
    }
    let status = gst.wait().map_err(|e| e.to_string())?;
    fed?;
    if !status.success() {
        return Err(fill(t("gst-launch-1.0 ended with {}"), &[&status]));
    }
    Ok(())
}

/// Frames to the encoder at FPS by the clock: each written as many times as the frames due since the last, so a
/// slow compositor or a still screen keeps the video's time the time that went by.
fn feed(
    screen: &mut Screen,
    mut frame: crate::shot::Frame,
    region: (i32, i32, i32, i32),
    (w, h): (u32, u32),
    out: &mut impl Write,
    stop: &AtomicBool,
) -> Result<(), String> {
    let begun = Instant::now();
    let mut sent = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let rows = frame.crop((0, 0, w, h)).ok_or("a frame short of the region")?;
        let due = (begun.elapsed().as_secs_f64() * FPS as f64) as u64 + 1;
        while sent < due {
            out.write_all(&rows).map_err(|e| fill(t("the encoder quit: {}"), &[&e]))?;
            sent += 1;
        }
        std::thread::sleep((begun + Duration::from_secs_f64(sent as f64 / FPS as f64)).saturating_duration_since(Instant::now()));
        frame = screen.frame(Some(region), true)?;
    }
    Ok(())
}

/// A size the encoder takes: its 4:2:0 chroma wants even sides, the odd pixel cut off.
fn even(w: u32, h: u32) -> Option<(u32, u32)> {
    let (w, h) = (w & !1, h & !1);
    (w >= 2 && h >= 2).then_some((w, h))
}

/// gst-launch-1.0's arguments: raw frames on stdin, of this size and format, encoded into an mp4; with audio,
/// the default sink's monitor beside them.
fn pipeline(w: u32, h: u32, format: &str, encoder: &str, file: &Path, audio: bool) -> Vec<String> {
    let mut a = format!(
        "-e fdsrc fd=0 ! rawvideoparse width={w} height={h} format={format} framerate={FPS}/1 ! videoconvert ! \
         {encoder} ! h264parse ! queue ! mp4mux name=mux ! filesink"
    );
    if audio {
        a += " pulsesrc device=@DEFAULT_MONITOR@ ! audioconvert ! audioresample ! avenc_aac ! queue ! mux.";
    }
    let mut args: Vec<String> = a.split(' ').map(String::from).collect();
    // the path one argument, whatever is in it (gst-launch escapes each)
    let at = args.iter().position(|x| x == "filesink").map_or(args.len(), |i| i + 1);
    args.insert(at, format!("location={}", file.display()));
    args
}

/// Time gone by, as m:ss, or h:mm:ss past an hour.
fn elapsed(s: u64) -> String {
    match s / 3600 {
        0 => format!("{}:{:02}", s / 60, s % 60),
        hr => format!("{hr}:{:02}:{:02}", s / 60 % 60, s % 60),
    }
}

/// The bar's block: a red dot and the time gone by while recording, a click the stop; hidden otherwise.
pub fn block() -> Block {
    let p = pill();
    p.append(&crate::style::label("●", "rec-dot"));
    let time = gtk4::Label::new(None);
    p.append(&time);
    let s = slot(&p);
    s.set_visible(false);
    let click = gtk4::GestureClick::new();
    click.connect_released(|_, _, _, _| toggle(false));
    p.add_controller(click);
    p.set_cursor_from_name(Some("pointer"));
    let s2 = s.clone();
    // ponytail: polled twice a second, the dot up to half a second late; a signal from start() if that shows
    glib::timeout_add_local(Duration::from_millis(500), move || {
        let since = REC.with(|r| r.on.borrow().as_ref().map(|(t, _)| *t));
        s2.set_visible(since.is_some());
        if let Some(t) = since {
            time.set_text(&elapsed(t.elapsed().as_secs()));
        }
        glib::ControlFlow::Continue
    });
    Block::new(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_even() {
        assert_eq!(even(1921, 1081), Some((1920, 1080)));
        assert_eq!(even(1920, 1080), Some((1920, 1080)));
        assert_eq!(even(3, 2), Some((2, 2)));
        assert_eq!(even(1, 100), None);
    }

    #[test]
    fn pipeline_built() {
        let file = std::path::PathBuf::from("/home/u/My Videos/a.mp4");
        let a = pipeline(640, 480, "bgrx", "vah264enc", &file, false);
        assert_eq!(
            a.join(" "),
            "-e fdsrc fd=0 ! rawvideoparse width=640 height=480 format=bgrx framerate=30/1 ! videoconvert ! \
             vah264enc ! h264parse ! queue ! mp4mux name=mux ! filesink location=/home/u/My Videos/a.mp4"
        );
        assert!(a.contains(&"location=/home/u/My Videos/a.mp4".to_string()));
        let a = pipeline(2, 2, "rgbx", "openh264enc", &file, true).join(" ");
        assert!(a.contains("filesink location=/home/u/My Videos/a.mp4 pulsesrc device=@DEFAULT_MONITOR@"));
        assert!(a.ends_with("avenc_aac ! queue ! mux."));
    }

    #[test]
    fn elapsed_said() {
        assert_eq!(elapsed(7), "0:07");
        assert_eq!(elapsed(605), "10:05");
        assert_eq!(elapsed(3725), "1:02:05");
    }
}
