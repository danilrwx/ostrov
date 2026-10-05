//! The clipboard's history, in place of cliphist (and wl-paste --watch, wl-copy): every selection made, text or
//! picture, kept in ~/.cache/ostrov/clipboard, the newest first, 200 at most, one copy of each; what a password
//! manager copies (its x-kde-passwordManagerHint) left out. Watched through wlr-data-control, the protocol
//! wl-paste uses, on a Wayland connection and thread of its own; an entry put back on the clipboard by ostrov
//! itself, its data handed to whoever pastes. The launcher in the bar shows it ($mod+Shift+v: ostrov clip).

use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1};

/// The most entries kept, the biggest one.
const KEEP: usize = 200;
const BIGGEST: usize = 8 << 20;

/// The text kinds, best first, and the pictures.
const TEXT: [&str; 5] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "TEXT", "STRING"];
const PICTURES: [(&str, &str); 2] = [("image/png", "png"), ("image/jpeg", "jpeg")];

fn dir() -> PathBuf {
    crate::hub::home().join(".cache/ostrov/clipboard")
}

/// The history's files, one an entry: ID-HASH.KIND (text, png, jpeg), the newest the highest id. File work under
/// one lock, the watcher's thread and GTK's both at it.
static FILES: Mutex<()> = Mutex::new(());

/// An entry: its id, its kind, its file.
struct Entry {
    id: u64,
    hash: String,
    kind: String,
    path: PathBuf,
}

fn entries() -> Vec<Entry> {
    let mut v: Vec<Entry> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|f| {
            let name = f.file_name().into_string().ok()?;
            let (stem, kind) = name.rsplit_once('.')?;
            let (id, hash) = stem.split_once('-')?;
            Some(Entry { id: id.parse().ok()?, hash: hash.into(), kind: kind.into(), path: f.path() })
        })
        .collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.id));
    v
}

/// The data kept, at the top; the same data already there moves up instead; past KEEP the oldest go.
fn keep(data: &[u8], kind: &str) {
    if data.is_empty() || data.len() > BIGGEST || (kind == "text" && data.iter().all(u8::is_ascii_whitespace)) {
        return;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    let hash = format!("{:016x}", h.finish());
    let _l = FILES.lock();
    let all = entries();
    let id = all.first().map_or(1, |e| e.id + 1);
    let _ = std::fs::create_dir_all(dir());
    let path = dir().join(format!("{id}-{hash}.{kind}"));
    if let Some(same) = all.iter().find(|e| e.hash == hash) {
        let _ = std::fs::rename(&same.path, &path);
    } else if std::fs::write(&path, data).is_err() {
        return;
    }
    for old in entries().iter().skip(KEEP) {
        let _ = std::fs::remove_file(&old.path);
    }
}

/// The history, newest first: each entry's id, whether a picture, and what the launcher shows of it.
pub fn list() -> Vec<(u64, bool, String)> {
    let _l = FILES.lock();
    entries()
        .into_iter()
        .map(|e| {
            let shown = if e.kind == "text" {
                let mut b = vec![0; 400];
                let n = std::fs::File::open(&e.path).and_then(|mut f| f.read(&mut b)).unwrap_or(0);
                String::from_utf8_lossy(&b[..n]).into_owned()
            } else {
                let kib = std::fs::metadata(&e.path).map_or(0, |m| m.len() / 1024);
                crate::i18n::fill(crate::i18n::t("[{} picture, {} KiB]"), &[&e.kind, &kib])
            };
            (e.id, e.kind != "text", shown)
        })
        .collect()
}

/// An entry's data, and whether a picture.
pub fn content(id: u64) -> Option<(bool, Vec<u8>)> {
    let _l = FILES.lock();
    let e = entries().into_iter().find(|e| e.id == id)?;
    Some((e.kind != "text", std::fs::read(&e.path).ok()?))
}

pub fn delete(id: u64) {
    let _l = FILES.lock();
    if let Some(e) = entries().into_iter().find(|e| e.id == id) {
        let _ = std::fs::remove_file(e.path);
    }
}

/// Data of a kind (text, png, jpeg) on the clipboard, ostrov its owner till another selection is made; the
/// watcher keeps it in the history as any selection.
pub fn put(data: Vec<u8>, kind: &str) {
    let Some(w) = WATCH.get() else { return };
    let mimes: Vec<&str> = if kind == "text" {
        TEXT.to_vec()
    } else {
        PICTURES.iter().filter(|(_, k)| *k == kind).map(|(m, _)| *m).collect()
    };
    let source = w.manager.create_data_source(&w.qh, Arc::new(data));
    for m in mimes {
        source.offer(m.into());
    }
    w.device.set_selection(Some(&source));
    let _ = w.conn.flush();
}

/// An entry back on the clipboard.
pub fn copy(id: u64) {
    let found = {
        let _l = FILES.lock();
        entries().into_iter().find(|e| e.id == id).and_then(|e| Some((std::fs::read(&e.path).ok()?, e.kind)))
    };
    if let Some((data, kind)) = found {
        put(data, &kind);
    }
}

/// What GTK's thread needs of the watcher's to put an entry back.
struct Watch {
    conn: Connection,
    qh: QueueHandle<Clip>,
    manager: ZwlrDataControlManagerV1,
    device: ZwlrDataControlDeviceV1,
}

static WATCH: OnceLock<Watch> = OnceLock::new();

pub fn start() {
    std::thread::spawn(|| {
        let Ok(conn) = Connection::connect_to_env() else { return };
        let Ok((globals, mut queue)) = registry_queue_init::<Clip>(&conn) else { return };
        let qh = queue.handle();
        let (Ok(manager), Ok(seat)) = (
            globals.bind::<ZwlrDataControlManagerV1, _, _>(&qh, 1..=2, ()),
            globals.bind::<WlSeat, _, _>(&qh, 1..=1, ()),
        ) else {
            eprintln!("ostrov: clipboard: no wlr-data-control");
            return;
        };
        let device = manager.get_data_device(&seat, &qh, ());
        let _ = WATCH.set(Watch { conn: conn.clone(), qh, manager, device });
        let mut clip = Clip;
        while queue.blocking_dispatch(&mut clip).is_ok() {}
    });
}

struct Clip;

/// An offer's kinds, as it tells them before it is the selection.
type Mimes = Mutex<Vec<String>>;

impl Dispatch<WlRegistry, GlobalListContents> for Clip {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlSeat, ()> for Clip {
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwlrDataControlManagerV1, ()> for Clip {
    fn event(_: &mut Self, _: &ZwlrDataControlManagerV1, _: <ZwlrDataControlManagerV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwlrDataControlOfferV1, Mimes> for Clip {
    fn event(_: &mut Self, _: &ZwlrDataControlOfferV1, e: zwlr_data_control_offer_v1::Event, mimes: &Mimes, _: &Connection, _: &QueueHandle<Self>) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = e
            && let Ok(mut m) = mimes.lock() {
                m.push(mime_type);
            }
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for Clip {
    fn event(_: &mut Self, _: &ZwlrDataControlDeviceV1, e: zwlr_data_control_device_v1::Event, _: &(), conn: &Connection, _: &QueueHandle<Self>) {
        // the selection's data read off this thread: its owner may be ostrov itself, answering on this one
        let zwlr_data_control_device_v1::Event::Selection { id: Some(offer) } = e else { return };
        let Some(mimes) = offer.data::<Mimes>().and_then(|m| m.lock().ok().map(|m| m.clone())) else { return };
        if mimes.iter().any(|m| m == "x-kde-passwordManagerHint") {
            offer.destroy();
            return;
        }
        let kind = TEXT
            .iter()
            .find(|t| mimes.iter().any(|m| m == *t))
            .map(|t| (*t, "text"))
            .or_else(|| PICTURES.iter().find(|(p, _)| mimes.iter().any(|m| m == p)).copied());
        let Some((mime, kind)) = kind else {
            offer.destroy();
            return;
        };
        let Ok((r, w)) = std::io::pipe() else { return };
        offer.receive(mime.into(), w.as_fd());
        drop(w);
        let _ = conn.flush();
        offer.destroy();
        std::thread::spawn(move || {
            let mut data = Vec::new();
            let mut r = r;
            if r.read_to_end(&mut data).is_ok() {
                keep(&data, kind);
            }
        });
    }

    event_created_child!(Clip, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, Mimes::default()),
    ]);
}

impl Dispatch<ZwlrDataControlSourceV1, Arc<Vec<u8>>> for Clip {
    fn event(_: &mut Self, source: &ZwlrDataControlSourceV1, e: zwlr_data_control_source_v1::Event, data: &Arc<Vec<u8>>, _: &Connection, _: &QueueHandle<Self>) {
        match e {
            zwlr_data_control_source_v1::Event::Send { fd, .. } => {
                let data = data.clone();
                std::thread::spawn(move || {
                    let mut f = std::fs::File::from(fd);
                    let _ = f.write_all(&data);
                });
            }
            zwlr_data_control_source_v1::Event::Cancelled => source.destroy(),
            _ => {}
        }
    }
}
