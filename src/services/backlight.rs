//! The backlight through sysfs and logind (backlight.go).
use super::{Ctx, Res, USAGE};

/// The panel's backlight device under /sys/class/backlight, None without one.
fn backlight() -> Option<std::path::PathBuf> {
    let mut dirs: Vec<_> = std::fs::read_dir("/sys/class/backlight").ok()?.flatten().map(|d| d.path()).collect();
    dirs.sort();
    dirs.into_iter().next()
}

/// A number from a sysfs file, the error worded as Go's strconv.Atoi words it.
fn read(dir: &std::path::Path, file: &str) -> Result<i64, String> {
    let s = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
    let s = s.trim();
    s.parse().map_err(|_| format!("strconv.Atoi: parsing {s:?}: invalid syntax"))
}

/// The backlight in percent of its maximum, -1 without one.
pub fn brightness() -> i64 {
    let Some(dir) = backlight() else { return -1 };
    match (read(&dir, "brightness"), read(&dir, "max_brightness")) {
        (Ok(cur), Ok(max)) if max != 0 => (cur * 100 + max / 2) / max,
        _ => -1,
    }
}

/// brightness PERCENT, through logind's SetBrightness, which the session's user may call without root; never all
/// the way down to 0, a black screen.
pub async fn cmd(c: &Ctx, args: &[&str]) -> Res {
    let [pct] = args else { return Err(USAGE.into()) };
    let pct: i64 = pct.parse().map_err(|_| USAGE.to_string())?;
    let dir = backlight().ok_or("no backlight")?;
    let max = read(&dir, "max_brightness")?;
    let value = (max * pct / 100).min(max).max(1) as u32;
    let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
    c.system
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1/session/auto",
            Some("org.freedesktop.login1.Session"),
            "SetBrightness",
            &("backlight", name, value),
        )
        .await
        .map(drop)
        .map_err(super::power::dbus_err)
}

#[cfg(test)]
mod tests {
    /// The state as wmd watch would print it: cargo test --release services_state -- --nocapture.
    #[tokio::test]
    async fn services_state() {
        let (system, session) = (zbus::Connection::system().await.unwrap(), zbus::Connection::session().await.unwrap());
        let c = super::Ctx { system, session };
        println!("{}", super::super::power::state(&c).await);
        println!("{}", super::super::battery::state(&c).await);
        println!("{}", super::brightness());
    }
}
