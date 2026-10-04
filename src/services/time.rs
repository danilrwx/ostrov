//! The local clock and the calendar's dates, for the weather's hours and the calendar's days: the local zone GLib's,
//! the dates by Howard Hinnant's algorithms.
use std::time::{SystemTime, UNIX_EPOCH};

/// Unix seconds as local ones: the local zone's offset then added (GLib's, its summer time and all).
pub fn local(secs: i64) -> i64 {
    secs + gtk4::glib::DateTime::from_unix_local(secs).map_or(0, |d| d.utc_offset().as_seconds())
}

/// Now, in nanoseconds since the epoch.
pub fn now_ns() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64)
}

/// The date of a day since the epoch: year, month, day (Hinnant's civil_from_days).
pub fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The day since the epoch of a date, civil's inverse (Hinnant's days_from_civil).
pub fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

/// Local seconds as "HH:MM".
pub fn hhmm(local: i64) -> String {
    format!("{:02}:{:02}", local.rem_euclid(86400) / 3600, local.rem_euclid(3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(civil(days(2026, 9, 23)), (2026, 9, 23));
        assert_eq!(civil(days(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(hhmm(7 * 3600 + 5 * 60), "07:05");
    }
}
