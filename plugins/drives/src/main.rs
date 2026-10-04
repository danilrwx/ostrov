//! ostrov's removable drives, as GNOME has them: the disks the user plugs in (USB sticks, SD cards, external
//! disks, phones' mass storage) found by lsblk, again whenever `udevadm monitor` says a block device changed (every
//! 3 s without udevadm); mounted, unmounted and powered off by udisksctl, which polkit lets the active session do
//! without root, and opened with gio. A drive plugged in is said in a toast, and mounted when `automount` says so.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::time::Duration;

use ostrov_plugin::{Host, Plugin, Value, fill, json, t};
use serde::{Deserialize, Serialize};

const ICON: &str = "drive-removable-media-symbolic";
/// The timer asking for lsblk again: the watcher's, and a job's when it is done.
const REFRESH: u32 = 1;
const USAGE: &str = "usage: ostrov plugin drives list | mount NAME | unmount NAME | eject NAME | open NAME";
/// What udisks cannot mount: encrypted, LVM's, swap.
const UNMOUNTABLE: [&str; 3] = ["crypto_LUKS", "LVM2_member", "swap"];

/// A block device as `lsblk -J -b` prints it, what is on it its children.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Block {
    name: String,
    path: String,
    label: Option<String>,
    size: Option<u64>,
    fstype: Option<String>,
    mountpoint: Option<String>,
    rm: bool,
    hotplug: bool,
    #[serde(rename = "type")]
    kind: String,
    model: Option<String>,
    vendor: Option<String>,
    tran: Option<String>,
    children: Vec<Block>,
}

/// A filesystem on a drive, its mountpoint "" while it is not mounted.
#[derive(Serialize, Clone, Debug, PartialEq)]
struct Volume {
    name: String,
    path: String,
    label: String,
    fstype: String,
    size: u64,
    mountpoint: String,
}

/// A drive plugged in, its title what the user knows it by: its first volume's label, else its vendor and model.
#[derive(Serialize, Clone, Debug, PartialEq)]
struct Drive {
    name: String,
    path: String,
    title: String,
    size: u64,
    volumes: Vec<Volume>,
}

impl Drive {
    fn mounted(&self) -> Vec<&str> {
        self.volumes.iter().map(|v| v.mountpoint.as_str()).filter(|m| !m.is_empty()).collect()
    }
}

/// The drives plugged in, of lsblk's JSON.
fn parse(json: &str) -> Result<Vec<Drive>, String> {
    #[derive(Deserialize)]
    struct Out {
        blockdevices: Vec<Block>,
    }
    let out: Out = serde_json::from_str(json).map_err(|e| format!("lsblk: {e}"))?;
    Ok(out.blockdevices.iter().filter_map(drive).collect())
}

fn volume(b: &Block) -> Option<Volume> {
    let fstype = b.fstype.clone().filter(|f| !UNMOUNTABLE.contains(&f.as_str()))?;
    Some(Volume {
        name: b.name.clone(),
        path: b.path.clone(),
        label: b.label.clone().unwrap_or_default(),
        fstype,
        size: b.size.unwrap_or(0),
        mountpoint: b.mountpoint.clone().unwrap_or_default(),
    })
}

/// A disk the user plugs in with something to mount on it; None for the machine's own disks (NVMe, zram, loop
/// devices) and an empty card reader.
fn drive(b: &Block) -> Option<Drive> {
    let tran = b.tran.as_deref().unwrap_or("");
    let removable = b.rm || b.hotplug || matches!(tran, "usb" | "mmc" | "sdio" | "ieee1394");
    if b.kind != "disk" || !removable || tran == "nvme" || b.name.starts_with("zram") {
        return None;
    }
    // a disk formatted whole, as SD cards often are, is its own one volume
    let volumes: Vec<Volume> = if b.fstype.is_some() {
        volume(b).into_iter().collect()
    } else {
        b.children.iter().filter(|c| c.kind == "part").filter_map(volume).collect()
    };
    if volumes.is_empty() {
        return None;
    }
    let made = [&b.vendor, &b.model].iter().filter_map(|s| s.as_deref()).map(str::trim).collect::<Vec<_>>();
    let title = volumes
        .iter()
        .map(|v| v.label.clone())
        .find(|l| !l.is_empty())
        .or_else(|| Some(made.join(" ")).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| b.name.clone());
    Some(Drive { name: b.name.clone(), path: b.path.clone(), title, size: b.size.unwrap_or(0), volumes })
}

fn lsblk() -> Result<Vec<Drive>, String> {
    const COLUMNS: &str = "NAME,PATH,LABEL,SIZE,FSTYPE,MOUNTPOINT,RM,HOTPLUG,TYPE,MODEL,VENDOR,TRAN";
    parse(&exec(&argv(&["lsblk", "-J", "-b", "-o", COLUMNS]))?)
}

/// Bytes as GNOME says them, in powers of 1000: "15.9 GB".
fn size(b: u64) -> String {
    let units = [t("B"), t("kB"), t("MB"), t("GB"), t("TB")];
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1000.0 && i < units.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 { format!("{b} {}", units[0]) } else { format!("{v:.1} {}", units[i]) }
}

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| a.to_string()).collect()
}

/// udisksctl's verb (mount, unmount, power-off) on a block device.
fn udisks(verb: &str, path: &str) -> Vec<String> {
    argv(&["udisksctl", verb, "-b", path])
}

/// A program run to its end: its output, or its error output as what went wrong.
fn exec(args: &[String]) -> Result<String, String> {
    let (prog, rest) = args.split_first().ok_or("nothing to run")?;
    let out = Command::new(prog).args(rest).stdin(Stdio::null()).output().map_err(|e| format!("{prog}: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if err.is_empty() { format!("{prog}: {}", out.status) } else { err })
}

/// Where udisksctl mount says it mounted: "Mounted /dev/sdb1 at /media/me/STICK" (older ones end in a dot).
fn mounted_at(out: &str) -> Option<String> {
    let (_, dir) = out.trim().split_once(" at ")?;
    Some(dir.trim_end_matches('.').to_string())
}

/// A directory opened in the file manager, by gio, else xdg-open.
fn open_dir(run: &mut dyn FnMut(&[String]) -> Result<String, String>, dir: &str) -> Result<(), String> {
    run(&argv(&["gio", "open", dir])).or_else(|_| run(&argv(&["xdg-open", dir]))).map(drop)
}

/// One of the verbs done on a drive, or on one of its volumes alone, each program through run: mount (and open
/// it when open is set), unmount, eject (all its volumes unmounted, then the drive powered off), open (the volume,
/// else the first, mounted first if it is not).
fn work(
    run: &mut dyn FnMut(&[String]) -> Result<String, String>,
    verb: &str,
    d: &Drive,
    vol: Option<&Volume>,
    open: bool,
) -> Result<(), String> {
    let vols = vol.map_or_else(|| d.volumes.clone(), |v| vec![v.clone()]);
    match verb {
        "mount" => {
            for v in vols.iter().filter(|v| v.mountpoint.is_empty()) {
                let out = run(&udisks("mount", &v.path))?;
                if let Some(dir) = mounted_at(&out).filter(|_| open) {
                    open_dir(run, &dir)?;
                }
            }
        }
        "unmount" => {
            for v in vols.iter().filter(|v| !v.mountpoint.is_empty()) {
                run(&udisks("unmount", &v.path))?;
            }
        }
        "eject" => {
            for v in d.volumes.iter().filter(|v| !v.mountpoint.is_empty()) {
                run(&udisks("unmount", &v.path))?;
            }
            run(&udisks("power-off", &d.path))?;
        }
        "open" => {
            let Some(v) = vols.first() else { return Ok(()) };
            let dir = if v.mountpoint.is_empty() {
                mounted_at(&run(&udisks("mount", &v.path))?).ok_or("udisksctl did not say where it mounted")?
            } else {
                v.mountpoint.clone()
            };
            open_dir(run, &dir)?;
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

/// A drive by its name, path or title, or one of its volumes by its name, path or label.
fn find<'a>(drives: &'a [Drive], name: &str) -> Option<(&'a Drive, Option<&'a Volume>)> {
    let drive = drives.iter().find(|d| [&d.name, &d.path, &d.title].iter().any(|s| *s == name));
    drive.map(|d| (d, None)).or_else(|| {
        drives.iter().find_map(|d| {
            let v = d.volumes.iter().find(|v| [&v.name, &v.path, &v.label].iter().any(|s| *s == name))?;
            Some((d, Some(v)))
        })
    })
}

/// Block devices' events, a burst of them refreshing once 300 ms after its last; polling every 3 s when udevadm
/// is not there or has gone. udevadm, outliving the plugin, dies of a broken pipe at its next event.
fn watch(host: Host) {
    let (tx, rx) = channel();
    let child = Command::new("udevadm")
        .args(["monitor", "--udev", "--subsystem-match=block"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    if let Some(out) = child.ok().and_then(|mut c| c.stdout.take()) {
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if line.starts_with("UDEV") && tx.send(()).is_err() {
                    return;
                }
            }
        });
    } else {
        drop(tx);
    }
    while rx.recv().is_ok() {
        while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
        host.set_timer(0, REFRESH);
    }
    host.log("udevadm monitor is not running: looking for drives every 3 s");
    loop {
        std::thread::sleep(Duration::from_secs(3));
        host.set_timer(0, REFRESH);
    }
}

struct Drives {
    drives: Vec<Drive>,
    /// whether lsblk has been read once: the drives found then were there before, not plugged in
    seen: bool,
    watching: bool,
}

impl Drives {
    fn refresh(&mut self, host: &Host) {
        let found = match lsblk() {
            Ok(f) => f,
            Err(e) => return host.log(e),
        };
        for d in found.iter().filter(|d| self.seen && !self.drives.iter().any(|o| o.name == d.name)) {
            let (h, title, body) = (host.clone(), fill(t("{} connected"), &[&d.title]), size(d.size));
            std::thread::spawn(move || {
                let _ = h.toast(&title, &body);
            });
            if host.setting("automount").unwrap_or(false) {
                self.spawn(host, "mount", d, None);
            }
        }
        self.seen = true;
        if found != self.drives {
            self.drives = found;
            host.kick();
        }
    }

    /// A verb done on a thread (udisks may wait for a disk's cache to be written, or polkit for the user), what
    /// went wrong in a toast, lsblk read again after.
    fn spawn(&self, host: &Host, verb: &str, d: &Drive, vol: Option<&Volume>) {
        let (h, verb, d, vol) = (host.clone(), verb.to_string(), d.clone(), vol.cloned());
        let open = host.setting("open_on_mount").unwrap_or(false);
        std::thread::spawn(move || {
            if let Err(e) = work(&mut |a| exec(a), &verb, &d, vol.as_ref(), open) {
                let _ = h.toast(t("Drives"), &e);
            }
            h.set_timer(0, REFRESH);
        });
    }

    fn act(&self, host: &Host, verb: &str, name: &str) -> Result<String, String> {
        let (d, vol) = find(&self.drives, name).ok_or_else(|| format!("no drive {name:?}"))?;
        self.spawn(host, verb, d, vol);
        Ok(format!("{verb} {}", vol.map_or(&d.name, |v| &v.name)))
    }
}

impl Plugin for Drives {
    fn on_config(&mut self, host: &Host, _: &Value) {
        host.set_settings_schema(&json!({"sections": [{"title": t("Drives"), "fields": [
            {"key": "automount", "title": t("Mount when plugged in"), "type": "bool", "default": false},
            {"key": "open_on_mount", "title": t("Open when mounted"), "type": "bool", "default": false,
             "help": t("in the file manager, after Mount or mounting when plugged in")},
        ]}]}));
        if !self.watching {
            self.watching = true;
            self.refresh(host);
            let h = host.clone();
            std::thread::spawn(move || watch(h));
        }
    }

    fn on_timer(&mut self, host: &Host, id: u32) {
        if id == REFRESH {
            self.refresh(host);
        }
    }

    fn state(&mut self, _: &Host) -> Value {
        json!(self.drives)
    }

    fn run(&mut self, host: &Host, args: &[&str], _: Option<&str>) -> Result<String, String> {
        match args {
            ["list"] => serde_json::to_string_pretty(&self.drives).map_err(|e| e.to_string()),
            [verb @ ("mount" | "unmount" | "eject" | "open"), name] => self.act(host, verb, name),
            _ => Err(USAGE.into()),
        }
    }

    fn render(&mut self, _: &Host, widget: &str) -> Option<Value> {
        let any = !self.drives.is_empty();
        if widget == "drives#bar" {
            return Some(json!({"type": "image", "icon": ICON, "active": any}));
        }
        let sub = match self.drives.as_slice() {
            [] => t("No drives").to_string(),
            [d] => format!("{} · {}", d.title, size(d.size)),
            ds => fill(t("{} drives"), &[&ds.len()]),
        };
        let mut menu = vec![];
        if !any {
            menu.push(json!({"type": "label", "class": "dim", "text": t("Nothing plugged in")}));
        }
        for d in &self.drives {
            let mounted = d.mounted();
            let state = match mounted.as_slice() {
                [] => t("not mounted").to_string(),
                m => fill(t("at {}"), &[&m.join(", ")]),
            };
            let all = mounted.len() == d.volumes.len();
            let (verb, label) = if all { ("unmount", t("Unmount")) } else { ("mount", t("Mount")) };
            menu.push(json!({"type": "row", "id": format!("open:{}", d.name), "icon": ICON, "text": d.title,
                "note": format!("{} · {state}", size(d.size))}));
            menu.push(json!({"type": "box", "orientation": "horizontal", "children": [
                {"type": "button", "id": format!("open:{}", d.name), "icon": "folder-open-symbolic",
                 "label": t("Open")},
                {"type": "button", "id": format!("{verb}:{}", d.name), "icon": "drive-harddisk-symbolic",
                 "label": label},
                {"type": "button", "id": format!("eject:{}", d.name), "icon": "media-eject-symbolic",
                 "label": t("Eject")},
            ]}));
        }
        Some(json!({"type": "toggle", "id": "drives", "icon": ICON, "title": t("Drives"), "sub": sub, "on": any,
            "menu": {"type": "box", "children": menu}}))
    }

    fn on_event(&mut self, host: &Host, _: &str, node: &str, _: &str, _: &str) {
        // the tile's switch shows whether a drive is there, not something to turn
        if let Some((verb, name)) = node.split_once(':') {
            let _ = self.act(host, verb, name);
        }
        host.kick();
    }
}

fn main() {
    ostrov_plugin::run(Drives { drives: vec![], seen: false, watching: false });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drives() {
        // this laptop: an NVMe disk with LUKS and LVM, nothing removable
        assert_eq!(parse(include_str!("../fixtures/laptop.json")), Ok(vec![]));
        // and a USB stick, an SD card formatted whole, an empty card reader, a loop device and zram plugged in
        let ds = parse(include_str!("../fixtures/usb.json")).expect("usb.json");
        let names: Vec<_> = ds.iter().map(|d| (d.name.as_str(), d.title.as_str())).collect();
        assert_eq!(names, [("sda", "STICK"), ("mmcblk0", "SDCARD")]);
        let stick = &ds[0];
        let vols: Vec<_> = stick.volumes.iter().map(|v| (v.name.as_str(), v.mountpoint.as_str())).collect();
        assert_eq!(vols, [("sda1", "/media/user/STICK"), ("sda2", "")], "swap left out");
        assert_eq!(size(stick.size), "15.9 GB");
        assert_eq!(size(512), "512 B");

        assert_eq!(find(&ds, "sda").map(|(d, v)| (d.name.as_str(), v.is_none())), Some(("sda", true)));
        assert_eq!(find(&ds, "/dev/sda1").and_then(|(_, v)| v).map(|v| v.name.as_str()), Some("sda1"));
        assert!(find(&ds, "nvme0n1").is_none());

        // the programs each verb runs, udisksctl answering as it does
        let plan = |verb: &str, name: &str, open: bool| {
            let (d, v) = find(&ds, name).expect(name);
            let mut ran = vec![];
            let r = work(
                &mut |a| {
                    ran.push(a.join(" "));
                    Ok(if a[1] == "mount" { format!("Mounted {} at /media/user/X.\n", a[3]) } else { String::new() })
                },
                verb,
                d,
                v,
                open,
            );
            (r, ran)
        };
        assert_eq!(plan("mount", "sda", true), (Ok(()), vec![
            "udisksctl mount -b /dev/sda2".into(), "gio open /media/user/X".into()]));
        assert_eq!(plan("unmount", "sda", false).1, ["udisksctl unmount -b /dev/sda1"]);
        let eject = ["udisksctl unmount -b /dev/sda1", "udisksctl power-off -b /dev/sda"];
        assert_eq!(plan("eject", "sda2", false).1, eject, "eject unmounts the whole drive");
        assert_eq!(plan("open", "sda", false).1, ["gio open /media/user/STICK"]);
        assert_eq!(plan("open", "mmcblk0", false).1, ["udisksctl mount -b /dev/mmcblk0", "gio open /media/user/X"]);
        assert_eq!(mounted_at("Mounted /dev/sdb1 at /media/me/My Stick"), Some("/media/me/My Stick".into()));
    }
}
