//! Where the battery stops charging (and starts again), the kernel's charge_control thresholds: kept below full,
//! a laptop that lives on its charger ages its battery slower. Written as the user, which the files allow only once
//! a udev rule gives them to a group of the user's (packaging/udev/90-ostrov-battery.rules): without it, said so.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::services::Res;

/// The battery with thresholds the kernel lets be set, if there is one.
fn battery() -> Option<PathBuf> {
    let dir = std::fs::read_dir("/sys/class/power_supply").ok()?;
    dir.flatten().map(|e| e.path()).find(|p| p.join("charge_control_end_threshold").exists())
}

fn read(p: &std::path::Path, f: &str) -> Option<u64> {
    std::fs::read_to_string(p.join(f)).ok()?.trim().parse().ok()
}

/// {end, start, writable}, null with no such battery.
pub fn state() -> Value {
    let Some(b) = battery() else { return Value::Null };
    let end = b.join("charge_control_end_threshold");
    let writable = std::fs::OpenOptions::new().write(true).open(&end).is_ok();
    json!({"end": read(&b, "charge_control_end_threshold"), "start": read(&b, "charge_control_start_threshold"),
        "writable": writable})
}

/// Charging stopped at end percent, started again 5 below it (100: charged full, as without a limit).
pub fn set(end: u64) -> Res {
    let b = battery().ok_or("this battery has no charge thresholds")?;
    let end = end.clamp(50, 100);
    let start = if end >= 100 { 95 } else { end.saturating_sub(5) };
    let write = |f: &str, v: u64| {
        std::fs::write(b.join(f), v.to_string()).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => {
                "not allowed: install packaging/udev/90-ostrov-battery.rules (ostrov doctor says)".to_string()
            }
            _ => e.to_string(),
        })
    };
    // the start below the end at every step: lowered first when the end comes down, last when it goes up
    let has_start = b.join("charge_control_start_threshold").exists();
    let cur = read(&b, "charge_control_end_threshold").unwrap_or(100);
    if has_start && end < cur {
        write("charge_control_start_threshold", start)?;
    }
    write("charge_control_end_threshold", end)?;
    if has_start && end >= cur {
        write("charge_control_start_threshold", start)?;
    }
    Ok(())
}
