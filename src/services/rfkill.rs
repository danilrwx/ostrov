//! The radios through /dev/rfkill.

use std::io::Write;

use super::Res;

/// rfkill types, as the kernel numbers them.
pub const WLAN: u8 = 1;
pub const BLUETOOTH: u8 = 2;

/// Soft-blocks or unblocks every radio of a type through /dev/rfkill, which logind opens to the session's user:
/// an rfkill_event with RFKILL_OP_CHANGE_ALL.
pub fn set_blocked(typ: u8, blocked: bool) -> Res {
    const OP_CHANGE_ALL: u8 = 3;
    let mut f =
        std::fs::OpenOptions::new().write(true).open("/dev/rfkill").map_err(|e| format!("open /dev/rfkill: {e}"))?;
    f.write_all(&[0, 0, 0, 0, typ, OP_CHANGE_ALL, u8::from(blocked), 0]).map_err(|e| e.to_string())
}

/// Whether any radio of the named type ("wlan", "bluetooth") is soft- or hard-blocked.
pub fn blocked(name: &str) -> bool {
    let Ok(dirs) = std::fs::read_dir("/sys/class/rfkill") else { return false };
    dirs.flatten().filter(|d| d.file_name().to_string_lossy().starts_with("rfkill")).any(|d| {
        let read = |f: &str| std::fs::read_to_string(d.path().join(f)).unwrap_or_default().trim().to_owned();
        read("type") == name && (read("soft") == "1" || read("hard") == "1")
    })
}
