//! The calendar's events for the calendar popup: a CalDAV account's (any server's: Example, iCloud, Fastmail,
//! Nextcloud...; Google's wants OAuth, not had here; the login, and an app password printed by a command, never
//! kept in the config), and calendars shared as .ics
//! links, as config.toml's [calendar] says. Fetched every 15 min and on `calendar refresh`; the events of this
//! month and a week either side of it, their repeats unrolled here, in local time.
//!
//! iCalendar is read by hand, as much of it as meetings use: a start and an end (a date for a day's event, UTC,
//! or a zone's clock), SUMMARY, LOCATION, RRULE's daily, weekly (on its days), monthly and yearly repeats with
//! INTERVAL, COUNT and UNTIL, EXDATE, and a repeat moved (RECURRENCE-ID) in place of the one it moves.
use std::sync::Mutex;
use std::time::Duration;

use gtk4::glib;
use serde_json::{json, Value};

use super::night::{civil, days, local, now_ns};
use super::{Kick, Res, USAGE};
use crate::config::Calendar;

/// The events last fetched, null while no calendar is set.
static LAST: Mutex<Value> = Mutex::new(Value::Null);

/// `calendar refresh`'s nudge to run.
static REFRESH: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// How much of a calendar is read at most: an .ics of years of meetings is a few MB.
const LIMIT: u64 = 64 << 20;

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
fn unfold(b: &[u8]) -> String {
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
fn iso(t: i64) -> String {
    let (y, m, d) = civil(t.div_euclid(86400));
    let s = t.rem_euclid(86400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// An iCalendar document's events between from and to (local seconds) onto out, in the state's shape, coloured
/// color where they say none of their own.
fn collect(ics: &str, color: &str, from: i64, to: i64, out: &mut Vec<Value>) {
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
            out.push(json!({
                "title": e.title,
                "start": iso(s),
                "end": iso(end),
                "all_day": all_day,
                "location": e.location,
                "color": if e.color.is_empty() { color } else { e.color.as_str() },
            }));
        }
    }
}

/// A node of an XML document: its name without its namespace's prefix, its children, its text.
#[derive(Default, Debug)]
struct Node {
    name: String,
    kids: Vec<Node>,
    text: String,
}

impl Node {
    /// The first node of the name under this one, depth first.
    fn find(&self, name: &str) -> Option<&Node> {
        self.kids.iter().find_map(|k| if k.name == name { Some(k) } else { k.find(name) })
    }

    /// Every outermost node of the name under this one.
    fn all(&self, name: &str) -> Vec<&Node> {
        self.kids.iter().flat_map(|k| if k.name == name { vec![k] } else { k.all(name) }).collect()
    }

    /// The text of the first node of the name under this one, "" if none.
    fn text_of(&self, name: &str) -> &str {
        self.find(name).map_or("", |n| n.text.trim())
    }
}

/// XML's entities undone: the five named ones and characters by number.
fn entities(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(j) = rest.find(';') else { break };
        let e = &rest[1..j];
        let c = match e {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => e
                .strip_prefix("#x")
                .map(|h| u32::from_str_radix(h, 16))
                .or_else(|| e.strip_prefix('#').map(str::parse))
                .and_then(Result::ok)
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[j + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A WebDAV answer as a tree, as much XML as a multistatus is: elements, text, CDATA; the prolog and comments
/// passed over.
fn xml(s: &str) -> Node {
    let mut stack = vec![Node::default()];
    let mut rest = s;
    let close = |stack: &mut Vec<Node>| {
        if stack.len() > 1
            && let Some(n) = stack.pop()
            && let Some(top) = stack.last_mut()
        {
            top.kids.push(n);
        }
    };
    while let Some(i) = rest.find('<') {
        if let Some(top) = stack.last_mut() {
            top.text.push_str(&entities(&rest[..i]));
        }
        rest = &rest[i..];
        if let Some(r) = rest.strip_prefix("<![CDATA[") {
            let end = r.find("]]>").unwrap_or(r.len());
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&r[..end]);
            }
            rest = r.get(end + 3..).unwrap_or("");
            continue;
        }
        let Some(j) = rest.find('>') else { break };
        let tag = &rest[1..j];
        rest = &rest[j + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if tag.starts_with('/') {
            close(&mut stack);
            continue;
        }
        let name = tag.trim_end_matches('/').split_whitespace().next().unwrap_or_default();
        let name = name.rsplit(':').next().unwrap_or_default().to_string();
        stack.push(Node { name, ..Node::default() });
        if tag.ends_with('/') {
            close(&mut stack);
        }
    }
    while stack.len() > 1 {
        close(&mut stack);
    }
    stack.pop().unwrap_or_default()
}

/// The href a PROPFIND's answer gives for prop (current-user-principal, calendar-home-set).
fn href(body: &str, prop: &str) -> Option<String> {
    let h = xml(body).find(prop)?.text_of("href").to_string();
    (!h.is_empty()).then_some(h)
}

/// The calendars of a calendar home's PROPFIND (Depth 1): their hrefs and colours, the collections that are
/// not calendars (the home, the scheduling inbox and outbox) left out.
fn calendars(body: &str) -> Vec<(String, String)> {
    let doc = xml(body);
    doc.all("response")
        .into_iter()
        .filter(|r| r.find("resourcetype").and_then(|t| t.find("calendar")).is_some())
        .filter_map(|r| {
            let href = r.kids.iter().find(|k| k.name == "href")?.text.trim().to_string();
            Some((href, r.text_of("calendar-color").to_string()))
        })
        .collect()
}

/// The iCalendar documents of a calendar-query REPORT's answer.
fn datas(body: &str) -> Vec<String> {
    xml(body).all("calendar-data").into_iter().map(|n| n.text.clone()).collect()
}

/// An href on base's server.
fn join(base: &str, href: &str) -> String {
    if href.starts_with("http") {
        return href.to_string();
    }
    let host_end = base.find("://").map_or(0, |i| i + 3);
    let origin = base[host_end..].find('/').map_or(base, |i| &base[..host_end + i]);
    format!("{origin}{href}")
}

fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    b.chunks(3)
        .flat_map(|c| {
            let n = c.iter().enumerate().fold(0u32, |n, (i, &x)| n | u32::from(x) << (16 - 8 * i));
            (0..4).map(move |i| if i <= c.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' })
        })
        .collect()
}

/// The app password, what password_command prints (its last line break off).
fn password(cmd: &str) -> Result<String, String> {
    let out = std::process::Command::new("sh").args(["-c", cmd]).output().map_err(|e| e.to_string())?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(format!("password_command: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end_matches(['\r', '\n']).to_string())
}

/// A WebDAV request (PROPFIND, REPORT) of an XML body, its answer's body; blocking.
fn dav(agent: &ureq::Agent, auth: &str, method: &str, url: &str, depth: &str, body: &str) -> Result<String, String> {
    let req = ureq::http::Request::builder()
        .method(method)
        .uri(url)
        .header("Authorization", auth)
        .header("Depth", depth)
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(body.to_string())
        .map_err(|e| e.to_string())?;
    let b = agent.run(req).and_then(|mut r| r.body_mut().with_config().limit(LIMIT).read_to_vec());
    Ok(unfold(&b.map_err(|e| format!("{method} {url}: {e}"))?))
}

/// The iCalendar documents of a CalDAV account's calendars with their colours, of the events between from and
/// to: the user's principal, its calendar home, the calendars in it, then each one's events in the span.
fn caldav(agent: &ureq::Agent, c: &Calendar, from: i64, to: i64) -> Result<Vec<(String, String)>, String> {
    let auth = format!("Basic {}", base64(format!("{}:{}", c.user, password(&c.password_command)?).as_bytes()));
    let dav = |method, url: &str, depth, body: &str| dav(agent, &auth, method, url, depth, body);
    const D: &str = r#"xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav""#;
    let root = c.caldav_url.as_str();
    let principal = format!("<d:propfind {D}><d:prop><d:current-user-principal/></d:prop></d:propfind>");
    let principal = href(&dav("PROPFIND", root, "0", &principal)?, "current-user-principal")
        .map_or(root.to_string(), |h| join(root, &h));
    let home = format!("<d:propfind {D}><d:prop><c:calendar-home-set/></d:prop></d:propfind>");
    let home = href(&dav("PROPFIND", &principal, "0", &home)?, "calendar-home-set");
    let home = home.map_or(principal, |h| join(root, &h));
    let list = format!(
        "<d:propfind {D} xmlns:a=\"http://apple.com/ns/ical/\"><d:prop><d:resourcetype/><d:displayname/>\
         <a:calendar-color/></d:prop></d:propfind>"
    );
    // the span in UTC, a day wider each side for any zone's offset
    let utc = |t: i64| {
        let s = iso(t).replace(['-', ':'], "");
        format!("{s}Z")
    };
    let query = format!(
        "<c:calendar-query {D}><d:prop><c:calendar-data/></d:prop><c:filter><c:comp-filter name=\"VCALENDAR\">\
         <c:comp-filter name=\"VEVENT\"><c:time-range start=\"{}\" end=\"{}\"/></c:comp-filter></c:comp-filter>\
         </c:filter></c:calendar-query>",
        utc(from - 86400),
        utc(to + 86400)
    );
    let mut out = Vec::new();
    for (href, color) in calendars(&dav("PROPFIND", &home, "1", &list)?) {
        let body = dav("REPORT", &join(root, &href), "1", &query)?;
        out.extend(datas(&body).into_iter().map(|ics| (unfold(ics.as_bytes()), color.clone())));
    }
    Ok(out)
}

/// The span shown: this month and a week either side, in local seconds.
fn span() -> (i64, i64) {
    let (y, m, _) = civil(local(now_ns().div_euclid(1_000_000_000)).div_euclid(86400));
    let next = if m == 12 { days(y + 1, 1, 1) } else { days(y, m + 1, 1) };
    ((days(y, m, 1) - 7) * 86400, (next + 7) * 86400)
}

/// Every calendar's events in the span, sorted by their starts; blocking, so for spawn_blocking. A calendar
/// that fails fails it all, so the last events stay rather than some of them.
fn fetch(c: &Calendar) -> Result<Value, String> {
    let (from, to) = span();
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .allow_non_standard_methods(true)
        .build()
        .new_agent();
    let mut docs = Vec::new();
    for url in &c.ics {
        // webcal:// is https:// to a calendar app
        let url = url.replacen("webcal://", "https://", 1);
        let b = agent.get(&url).call().and_then(|mut r| r.body_mut().with_config().limit(LIMIT).read_to_vec());
        docs.push((unfold(&b.map_err(|e| format!("{url}: {e}"))?), String::new()));
    }
    if !c.caldav_url.is_empty() && !c.user.is_empty() {
        docs.extend(caldav(&agent, c, from, to)?);
    }
    let mut events = Vec::new();
    for (ics, color) in &docs {
        collect(ics, color, from, to, &mut events);
    }
    events.sort_by(|a, b| a["start"].as_str().cmp(&b["start"].as_str()));
    Ok(Value::Array(events))
}

/// The events last fetched: [{title, start, end, all_day, location, color}], null while there is no calendar.
pub fn state() -> Value {
    LAST.lock().map(|w| w.clone()).unwrap_or_default()
}

/// calendar refresh: the calendars fetched now.
pub async fn cmd(args: &[&str]) -> Res {
    match args {
        ["refresh"] => {
            REFRESH.notify_one();
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

/// The events now and every 15 min (sooner, 1 min, after a failure, said on stderr) or on a refresh, a kick each
/// time; the config read anew each time, so a calendar added needs no restart.
pub async fn run(kick: Kick) {
    loop {
        let c = crate::config::load().calendar;
        let mut wait = Duration::from_secs(15 * 60);
        let got = if c.ics.is_empty() && (c.caldav_url.is_empty() || c.user.is_empty()) {
            Ok(Value::Null)
        } else {
            tokio::task::spawn_blocking(move || fetch(&c)).await.map_err(|e| e.to_string()).and_then(|r| r)
        };
        match got {
            Ok(v) => {
                if let Ok(mut last) = LAST.lock() {
                    *last = v;
                }
                let _ = kick.send(()).await;
            }
            Err(e) => {
                eprintln!("ostrov: calendar: {e}");
                wait = Duration::from_secs(60);
            }
        }
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            () = REFRESH.notified() => {}
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
        let got: Vec<(&str, &str, &str)> = out
            .iter()
            .map(|e| (e["title"].as_str().unwrap(), e["start"].as_str().unwrap(), e["color"].as_str().unwrap()))
            .collect();
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
        assert_eq!(out[0]["end"], "2026-10-10T00:00:00");
        assert_eq!(out[0]["all_day"], true);
        assert_eq!(out[1]["end"], at(2026, 10, 5, 7, 30));
        assert_eq!(out[1]["location"], "Room 1");
        assert_eq!(out[1]["all_day"], false);
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

    const MULTISTATUS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://apple.com/ns/ical/">
  <D:response>
    <D:href>/calendars/me%40example.com/</D:href>
    <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype><D:displayname>me</D:displayname>
    </D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
  <D:response>
    <D:href>/calendars/me%40example.com/events-1/</D:href>
    <D:propstat><D:prop><D:resourcetype><D:collection/><C:calendar/></D:resourcetype>
    <D:displayname>Work</D:displayname><A:calendar-color>#49b0ffff</A:calendar-color></D:prop>
    <D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
</D:multistatus>"#;

    const REPORT: &str = "<d:multistatus xmlns:d=\"DAV:\" xmlns:cal=\"urn:ietf:params:xml:ns:caldav\"><d:response>\
<d:href>/a.ics</d:href><d:propstat><d:prop><cal:calendar-data>BEGIN:VCALENDAR&#13;\nBEGIN:VEVENT&#13;\n\
SUMMARY:Q&amp;A&#13;\nEND:VEVENT&#13;\nEND:VCALENDAR&#13;\n</cal:calendar-data></d:prop></d:propstat></d:response>\
<d:response><d:href>/b.ics</d:href><d:propstat><d:prop><cal:calendar-data><![CDATA[BEGIN:VCALENDAR\n\
SUMMARY:<b>\nEND:VCALENDAR]]></cal:calendar-data></d:prop></d:propstat></d:response></d:multistatus>";

    #[test]
    fn caldav_xml() {
        let principal = r#"<multistatus xmlns="DAV:"><response><href>/</href><propstat><prop><current-user-principal>
            <href>/principals/users/me%40example.com/</href></current-user-principal></prop></propstat></response>
            </multistatus>"#;
        assert_eq!(href(principal, "current-user-principal").as_deref(), Some("/principals/users/me%40example.com/"));
        assert_eq!(href(principal, "calendar-home-set"), None);
        assert_eq!(calendars(MULTISTATUS), vec![("/calendars/me%40example.com/events-1/".into(), "#49b0ffff".into())]);
        let d = datas(REPORT);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0], "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:Q&A\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n");
        assert!(d[1].contains("SUMMARY:<b>"));
        assert_eq!(join("https://caldav.example.com", "/calendars/x/"), "https://caldav.example.com/calendars/x/");
        assert_eq!(join("https://caldav.example.com/", "/c/"), "https://caldav.example.com/c/");
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    /// The calendars fetched once as the config says: cargo test -- --ignored --nocapture calendar_once
    #[test]
    #[ignore]
    fn calendar_once() {
        println!("{}", fetch(&crate::config::load().calendar).expect("fetch"));
    }
}
