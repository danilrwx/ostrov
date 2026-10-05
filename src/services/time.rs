//! The local clock and the calendar's dates, for the weather's hours and the calendar's days, and the time as the
//! clock's badge says it (here, not in modules/clock, for ostrov-ctl's status too, which links no GTK): the local
//! zone GLib's, the dates by Howard Hinnant's algorithms.
use std::time::{SystemTime, UNIX_EPOCH};

use gio::glib;

use crate::i18n::t;

/// Unix seconds as local ones: the local zone's offset then added (GLib's, its summer time and all).
pub fn local(secs: i64) -> i64 {
    secs + glib::DateTime::from_unix_local(secs).map_or(0, |d| d.utc_offset().as_seconds())
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

const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

/// The time as the format says, its day and month names (%A %a %B %b) in ostrov's language: glib's would be
/// LC_TIME's, English under a locale apart from the language set. A short name is the full one's first three
/// letters in English; %B a month's as a date says it (its genitive in Russian, "3 октября").
pub fn format_time(now: &glib::DateTime, fmt: &str) -> String {
    let day = DAYS[(now.day_of_week() as usize).clamp(1, 7) - 1];
    let month = MONTHS[(now.month() as usize).clamp(1, 12) - 1];
    let mut out = String::with_capacity(fmt.len() + 16);
    let mut chars = fmt.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('A') => out.push_str(t(day)),
            Some('a') => out.push_str(t(&day[..3])),
            Some('B') => out.push_str(t(month)),
            Some('b') => out.push_str(t(&month[..3])),
            Some(o) => out.extend(['%', o]),
            None => out.push('%'),
        }
    }
    now.format(&out).map(|s| s.to_string()).unwrap_or_default()
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

    #[test]
    fn names_put_in_and_the_rest_left_to_glib() {
        let at = glib::DateTime::from_utc(2026, 10, 3, 13, 31, 0.0).unwrap();
        let want = format!("{} {} 3  13:31 %a", t("Sat"), t("Oct"));
        assert_eq!(format_time(&at, "%a %b %-d  %H:%M %%a"), want);
        assert_eq!(format_time(&at, "%A %B"), format!("{} {}", t("Saturday"), t("October")));
    }
}
