//! iCalendar read by hand, as much of it as meetings use: a start and an end (a date for a day's event, UTC, or a
//! zone's clock), SUMMARY, LOCATION, RRULE's daily, weekly (on its days), monthly and yearly repeats with
//! INTERVAL, COUNT and UNTIL, EXDATE, and a repeat moved (RECURRENCE-ID) in place of the one it moves; the events
//! between two times unrolled in local time.

use std::time::{SystemTime, UNIX_EPOCH};

use ostrov_plugin::CalendarEvent;

/// Local seconds of a Unix time: as if the local clock were UTC's.
pub fn local(secs: i64) -> i64 {
    secs + glib::DateTime::from_unix_local(secs).map_or(0, |d| d.utc_offset().as_seconds())
}

/// Now, local seconds.
pub fn now() -> i64 {
    local(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64))
}

/// The date of a day since the epoch: year, month, day (Howard Hinnant's civil_from_days).
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

/// Whose clock a time of iCalendar's is on.
#[derive(Clone, PartialEq, Debug)]
enum Zone {
    /// A date alone: a day's event, the day local.
    Date,
    Utc,
    /// The machine's own, as a time without a zone is.
    Floating,
    /// A zone's, by its IANA name (TZID).
    Tz(String),
}

/// A time as iCalendar writes it: its clock in seconds since the epoch, as if on UTC's, and whose clock that is.
#[derive(Clone, Debug)]
struct When {
    wall: i64,
    zone: Zone,
}

impl When {
    /// The time on the local clock, in seconds as night's local gives them. A zone GLib does not know (a
    /// Windows name) is taken for the local one.
    fn local(&self) -> i64 {
        match &self.zone {
            Zone::Utc => local(self.wall),
            Zone::Tz(id) => zoned(id, self.wall).map_or(self.wall, local),
            _ => self.wall,
        }
    }
}

/// The Unix time of a clock reading in zone id.
fn zoned(id: &str, wall: i64) -> Option<i64> {
    let tz = glib::TimeZone::from_identifier(Some(id))?;
    let (y, m, d) = civil(wall.div_euclid(86400));
    let s = wall.rem_euclid(86400);
    let (h, mi) = ((s / 3600) as i32, (s / 60 % 60) as i32);
    glib::DateTime::new(&tz, y as i32, m as i32, d as i32, h, mi, (s % 60) as f64).ok().map(|t| t.to_unix())
}

/// A parameter of a property's (TZID, VALUE), its quotes off.
fn param<'a>(params: &'a str, key: &str) -> Option<&'a str> {
    params.split(';').find_map(|p| {
        let (k, v) = p.split_once('=')?;
        k.trim().eq_ignore_ascii_case(key).then(|| v.trim().trim_matches('"'))
    })
}

/// A DATE or DATE-TIME value: 20261005, 20261005T100000Z, or 20261005T100000 on params' TZID or none.
fn when(v: &str, params: &str) -> Option<When> {
    let v = v.trim();
    let n = |r: std::ops::Range<usize>| v.get(r)?.parse::<i64>().ok();
    let day = days(n(0..4)?, n(4..6)?, n(6..8)?) * 86400;
    if v.len() == 8 {
        return Some(When { wall: day, zone: Zone::Date });
    }
    let wall = day + n(9..11)? * 3600 + n(11..13)? * 60 + n(13..15)?;
    let zone = match param(params, "TZID") {
        _ if v.ends_with('Z') => Zone::Utc,
        Some(id) => Zone::Tz(id.to_string()),
        None => Zone::Floating,
    };
    Some(When { wall, zone })
}

/// Lines folded (a line break and a space or tab to go on) put back whole. On bytes: a fold may split a
/// character's UTF-8.
pub fn unfold(b: &[u8]) -> String {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let skip = match &b[i..] {
            [b'\r', b'\n', b' ' | b'\t', ..] => 3,
            [b'\n', b' ' | b'\t', ..] => 2,
            _ => 0,
        };
        if skip == 0 {
            out.push(b[i]);
            i += 1;
        } else {
            i += skip;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A TEXT value's escapes undone.
fn text(v: &str) -> String {
    let mut out = String::new();
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(c) => out.push(c),
            None => {}
        }
    }
    out
}

#[derive(Default)]
struct Event {
    uid: String,
    title: String,
    location: String,
    color: String,
    start: Option<When>,
    end: Option<When>,
    rrule: String,
    exdates: Vec<i64>,
    moved: Option<i64>,
    cancelled: bool,
}

/// An iCalendar document's events, and its calendar's colour (X-APPLE-CALENDAR-COLOR or COLOR) if it says one.
/// What a VEVENT holds (its alarms) and the zones' definitions are passed over.
fn parse(ics: &str) -> (Vec<Event>, String) {
    let (mut events, mut color) = (Vec::new(), String::new());
    let mut stack: Vec<String> = Vec::new();
    let mut ev = Event::default();
    for line in ics.lines() {
        // the value after the first colon not in a quoted parameter
        let mut quoted = false;
        let Some(i) = line.find(|c| {
            quoted ^= c == '"';
            c == ':' && !quoted
        }) else {
            continue;
        };
        let (head, value) = (&line[..i], &line[i + 1..]);
        let (name, params) = head.split_once(';').unwrap_or((head, ""));
        let name = name.to_ascii_uppercase();
        match name.as_str() {
            "BEGIN" => {
                if value.eq_ignore_ascii_case("VEVENT") {
                    ev = Event::default();
                }
                stack.push(value.to_ascii_uppercase());
                continue;
            }
            "END" => {
                if stack.pop().as_deref() == Some("VEVENT") {
                    events.push(std::mem::take(&mut ev));
                }
                continue;
            }
            _ => {}
        }
        match stack.last().map(String::as_str) {
            Some("VCALENDAR") if name == "X-APPLE-CALENDAR-COLOR" || name == "COLOR" => color = value.to_string(),
            Some("VEVENT") => match name.as_str() {
                "UID" => ev.uid = value.to_string(),
                "SUMMARY" => ev.title = text(value),
                "LOCATION" => ev.location = text(value),
                "COLOR" => ev.color = value.to_string(),
                "DTSTART" => ev.start = when(value, params),
                "DTEND" => ev.end = when(value, params),
                "RRULE" => ev.rrule = value.to_string(),
                "RECURRENCE-ID" => ev.moved = when(value, params).map(|w| w.local()),
                "STATUS" => ev.cancelled = value.eq_ignore_ascii_case("CANCELLED"),
                "EXDATE" => {
                    ev.exdates.extend(value.split(',').filter_map(|v| when(v, params)).map(|w| w.local()));
                }
                _ => {}
            },
            _ => {}
        }
    }
    (events, color)
}

/// A rule's part (FREQ, COUNT, ...).
fn part<'a>(rule: &'a str, key: &str) -> Option<&'a str> {
    rule.split(';').find_map(|p| p.split_once('=').filter(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v))
}

/// The day of a date if it is one (not February 30th).
fn date(y: i64, m: i64, d: i64) -> Option<i64> {
    let day = days(y, m, d);
    (civil(day) == (y, m, d)).then_some(day)
}

/// The starts, local, of an event's occurrences from the first to the first at or past `to`, or to its rule's
/// COUNT or UNTIL; a rule of another FREQ, or none, is the first alone.
fn starts(start: &When, rule: &str, to: i64) -> Vec<i64> {
    let freq = part(rule, "FREQ").unwrap_or_default().to_ascii_uppercase();
    if !matches!(freq.as_str(), "DAILY" | "WEEKLY" | "MONTHLY" | "YEARLY") {
        return vec![start.local()];
    }
    let iv = part(rule, "INTERVAL").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1).max(1);
    let count = part(rule, "COUNT").and_then(|v| v.parse::<usize>().ok()).unwrap_or(usize::MAX);
    // UNTIL a date takes in the whole of its day
    let until = part(rule, "UNTIL").and_then(|v| when(v, "")).map_or(i64::MAX, |u| match u.zone {
        Zone::Date => u.local() + 86399,
        _ => u.local(),
    });
    let (first, tod) = (start.wall.div_euclid(86400), start.wall.rem_euclid(86400));
    let (y0, m0, d0) = civil(first);
    // weekdays from Monday 0; 1970-01-01 was a Thursday
    let weekday = |day: i64| (day + 3).rem_euclid(7);
    let mut on: Vec<i64> = part(rule, "BYDAY")
        .unwrap_or_default()
        .split(',')
        .filter_map(|d| {
            let d = d.trim_start_matches(|c: char| c.is_ascii_digit() || c == '+' || c == '-');
            ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].iter().position(|w| d.eq_ignore_ascii_case(w))
        })
        .map(|w| w as i64)
        .collect();
    if on.is_empty() {
        on.push(weekday(first));
    }
    on.sort_unstable();
    let mut out = Vec::new();
    // ponytail: walks every period from the first, so a daily rule from years ago costs a few thousand steps
    for k in 0..100_000 {
        let days: Vec<i64> = match freq.as_str() {
            "DAILY" => vec![first + k * iv],
            "WEEKLY" => {
                let monday = first - weekday(first) + 7 * k * iv;
                on.iter().map(|w| monday + w).filter(|&d| d >= first).collect()
            }
            "MONTHLY" => {
                let mi = m0 - 1 + k * iv;
                date(y0 + mi.div_euclid(12), mi.rem_euclid(12) + 1, d0).into_iter().collect()
            }
            _ => date(y0 + k * iv, m0, d0).into_iter().collect(),
        };
        for day in days {
            let s = When { wall: day * 86400 + tod, zone: start.zone.clone() }.local();
            if out.len() >= count || s > until || s >= to {
                return out;
            }
            out.push(s);
        }
    }
    out
}

/// Local seconds as ISO 8601, "2026-10-05T10:00:00".
pub fn iso(t: i64) -> String {
    let (y, m, d) = civil(t.div_euclid(86400));
    let s = t.rem_euclid(86400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// An iCalendar document's events between from and to (local seconds) onto out, coloured color where they say
/// none of their own.
pub fn collect(ics: &str, color: &str, from: i64, to: i64, out: &mut Vec<CalendarEvent>) {
    let (events, own) = parse(ics);
    let color = if own.is_empty() { color } else { own.as_str() };
    let moved: Vec<(&str, i64)> = events.iter().filter_map(|e| Some((e.uid.as_str(), e.moved?))).collect();
    for e in events.iter().filter(|e| !e.cancelled) {
        let Some(start) = &e.start else { continue };
        let all_day = start.zone == Zone::Date;
        let length = e.end.as_ref().map_or(if all_day { 86400 } else { 0 }, |end| end.wall - start.wall);
        let rule = if e.moved.is_some() { "" } else { e.rrule.as_str() };
        for s in starts(start, rule, to) {
            if e.exdates.contains(&s) || (e.moved.is_none() && moved.contains(&(e.uid.as_str(), s))) {
                continue;
            }
            // the end on the start's clock, so a repeat's hour stays across a change of summer time
            let end = When { wall: s - start.local() + start.wall + length, zone: start.zone.clone() }.local();
            if end.max(s + 1) <= from {
                continue;
            }
            out.push(CalendarEvent {
                title: e.title.clone(),
                start: iso(s),
                end: iso(end),
                all_day,
                location: e.location.clone(),
                color: if e.color.is_empty() { color } else { e.color.as_str() }.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A local time of a UTC one, as ISO.
    fn at(y: i64, m: i64, d: i64, h: i64, mi: i64) -> String {
        iso(local(days(y, m, d) * 86400 + h * 3600 + mi * 60))
    }

    const ICS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nX-APPLE-CALENDAR-COLOR:#ff8800\r\n\
BEGIN:VTIMEZONE\r\nTZID:Europe/Moscow\r\nBEGIN:STANDARD\r\nDTSTART:19700101T000000\r\nTZOFFSETFROM:+0300\r\n\
TZOFFSETTO:+0300\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
BEGIN:VEVENT\r\nUID:day\r\nDTSTART;VALUE=DATE:20261009\r\nDTEND;VALUE=DATE:20261010\r\n\
SUMMARY:Holiday\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:standup\r\nDTSTART;TZID=Europe/Moscow:20261005T100000\r\n\
DTEND;TZID=\"Europe/Moscow\":20261005T103000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=6\r\n\
EXDATE;TZID=Europe/Moscow:20261007T100000\r\nSUMMARY:Stand\r\n  up\\, daily\r\nLOCATION:Room 1\r\n\
BEGIN:VALARM\r\nACTION:DISPLAY\r\nSUMMARY:alarm\r\nTRIGGER:-PT5M\r\nEND:VALARM\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:standup\r\nRECURRENCE-ID;TZID=Europe/Moscow:20261012T100000\r\n\
DTSTART;TZID=Europe/Moscow:20261012T120000\r\nDTEND;TZID=Europe/Moscow:20261012T123000\r\n\
SUMMARY:Stand up\\, moved\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:z\r\nDTSTART:20261020T150000Z\r\nDTEND:20261020T160000Z\r\nSUMMARY:Call\r\n\
COLOR:red\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn ics() {
        let mut out = Vec::new();
        let (from, to) = (days(2026, 9, 24) * 86400, days(2026, 11, 8) * 86400);
        collect(&unfold(ICS.as_bytes()), "", from, to, &mut out);
        let got: Vec<(&str, &str, &str)> =
            out.iter().map(|e| (e.title.as_str(), e.start.as_str(), e.color.as_str())).collect();
        let (s5, s12, s14) = (at(2026, 10, 5, 7, 0), at(2026, 10, 12, 9, 0), at(2026, 10, 14, 7, 0));
        let (s19, s21, call) = (at(2026, 10, 19, 7, 0), at(2026, 10, 21, 7, 0), at(2026, 10, 20, 15, 0));
        let want = vec![
            ("Holiday", "2026-10-09T00:00:00", "#ff8800"),
            ("Stand up, daily", s5.as_str(), "#ff8800"),
            ("Stand up, daily", s14.as_str(), "#ff8800"),
            ("Stand up, daily", s19.as_str(), "#ff8800"),
            ("Stand up, daily", s21.as_str(), "#ff8800"),
            ("Stand up, moved", s12.as_str(), "#ff8800"),
            ("Call", call.as_str(), "red"),
        ];
        assert_eq!(got, want);
        assert_eq!((out[0].end.as_str(), out[0].all_day), ("2026-10-10T00:00:00", true));
        assert_eq!(out[1].end, at(2026, 10, 5, 7, 30));
        assert_eq!((out[1].location.as_str(), out[1].all_day), ("Room 1", false));
    }

    #[test]
    fn rules() {
        let start = when("20260131T090000", "").unwrap();
        let s = |rule| starts(&start, rule, days(2027, 1, 1) * 86400);
        let d = |m, dd| days(2026, m, dd) * 86400 + 9 * 3600;
        // the 31st of the months that have one
        assert_eq!(s("FREQ=MONTHLY;COUNT=3"), vec![d(1, 31), d(3, 31), d(5, 31)]);
        assert_eq!(s("FREQ=DAILY;INTERVAL=2;UNTIL=20260204T090000"), vec![d(1, 31), d(2, 2), d(2, 4)]);
        assert_eq!(s("FREQ=YEARLY").len(), 1);
    }
}
