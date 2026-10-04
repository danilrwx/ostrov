//! The radios through /dev/rfkill.

use std::io::Write;
use std::path::Path;

use super::Res;

/// rfkill types, as the kernel numbers them: ALL every radio, WLAN, BLUETOOTH, the rest by name (kind).
pub const ALL: u8 = 0;
pub const WLAN: u8 = 1;
pub const BLUETOOTH: u8 = 2;

/// A type's number from its name in /sys/class/rfkill/*/type.
pub fn kind(name: &str) -> Option<u8> {
    let names = ["all", "wlan", "bluetooth", "uwb", "wimax", "wwan", "gps", "fm", "nfc"];
    names.iter().position(|n| *n == name).map(|i| i as u8)
}

/// Soft-blocks or unblocks every radio of a type through /dev/rfkill, which logind opens to the session's user:
/// an rfkill_event with RFKILL_OP_CHANGE_ALL.
pub fn set_blocked(typ: u8, blocked: bool) -> Res {
    const OP_CHANGE_ALL: u8 = 3;
    let mut f =
        std::fs::OpenOptions::new().write(true).open("/dev/rfkill").map_err(|e| format!("open /dev/rfkill: {e}"))?;
    f.write_all(&[0, 0, 0, 0, typ, OP_CHANGE_ALL, u8::from(blocked), 0]).map_err(|e| e.to_string())
}

/// A radio: its type's name ("wlan"), whether soft-blocked, whether hard-blocked (its switch).
pub struct Radio {
    pub kind: String,
    pub soft: bool,
    pub hard: bool,
}

/// The radios now.
pub fn radios() -> Vec<Radio> {
    radios_in(Path::new("/sys/class/rfkill"))
}

/// The radios a directory like /sys/class/rfkill lists, a rfkillN each.
fn radios_in(dir: &Path) -> Vec<Radio> {
    let Ok(dirs) = std::fs::read_dir(dir) else { return vec![] };
    let mut out: Vec<(String, Radio)> = dirs
        .flatten()
        .filter(|d| d.file_name().to_string_lossy().starts_with("rfkill"))
        .map(|d| {
            let read = |f: &str| std::fs::read_to_string(d.path().join(f)).unwrap_or_default().trim().to_owned();
            let r = Radio { kind: read("type"), soft: read("soft") == "1", hard: read("hard") == "1" };
            (d.file_name().to_string_lossy().into_owned(), r)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter().map(|(_, r)| r).collect()
}

/// Whether any radio of the named type ("wlan", "bluetooth") is soft- or hard-blocked.
pub fn blocked(name: &str) -> bool {
    radios().iter().any(|r| r.kind == name && (r.soft || r.hard))
}

#[cfg(test)]
mod tests {
    #[test]
    fn radios_in() {
        let dir = std::env::temp_dir().join(format!("ostrov-rfkill-{}", std::process::id()));
        for (n, kind, soft, hard) in [("rfkill0", "wlan", "0", "0"), ("rfkill1", "bluetooth", "1", "0")] {
            let d = dir.join(n);
            std::fs::create_dir_all(&d).unwrap();
            for (f, v) in [("type", kind), ("soft", soft), ("hard", hard)] {
                std::fs::write(d.join(f), format!("{v}\n")).unwrap();
            }
        }
        std::fs::create_dir_all(dir.join("other")).unwrap();
        let r = super::radios_in(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        let got: Vec<(&str, bool, bool)> = r.iter().map(|r| (r.kind.as_str(), r.soft, r.hard)).collect();
        assert_eq!(got, [("wlan", false, false), ("bluetooth", true, false)]);
        assert_eq!(super::kind("wwan"), Some(5));
        assert_eq!(super::kind("bogus"), None);
    }
}
