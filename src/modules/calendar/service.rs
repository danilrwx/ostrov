//! The calendar's events for the month and Coming Up: its sources', the plugins that are calendars (Source; a
//! CalDAV account and .ics links are the official plugin caldav's). Asked every 15 min, on `calendar refresh` and
//! as a source comes or goes, for the events of this month and a week either side of it, in local time.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::backend::BoxFut;
use crate::services::time::{civil, days, local, now_ns};
use crate::services::{Kick, Res};

/// The events last asked for, null while there is no calendar.
static LAST: Mutex<Value> = Mutex::new(Value::Null);

/// `calendar refresh`'s nudge to run.
static REFRESH: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Local seconds as ISO 8601, "2026-10-05T10:00:00".
fn iso(t: i64) -> String {
    let (y, m, d) = civil(t.div_euclid(86400));
    let s = t.rem_euclid(86400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// The span shown: this month and a week either side, in local seconds.
fn span() -> (i64, i64) {
    let (y, m, _) = civil(local(now_ns().div_euclid(1_000_000_000)).div_euclid(86400));
    let next = if m == 12 { days(y + 1, 1, 1) } else { days(y, m + 1, 1) };
    ((days(y, m, 1) - 7) * 86400, (next + 7) * 86400)
}

/// The events last asked for: [{title, start, end, all_day, location, color}], null while there is no calendar.
pub fn state() -> Value {
    LAST.lock().map(|w| w.clone()).unwrap_or_default()
}

/// calendar refresh: the sources asked again now.
pub async fn cmd(args: &[&str]) -> Res {
    match args {
        ["refresh"] => {
            REFRESH.notify_one();
            Ok(())
        }
        _ => Err(super::MODULE.usage()),
    }
}

/// A calendar: a plugin's (`calendar = true`), asked for its events between two local ISO
/// times, a JSON list in the state's shape. Its call may run anywhere (a plugin's on the plugins' runtime), its
/// answer awaited here.
pub type Source = Arc<dyn Fn(String, String) -> BoxFut<Result<Value, String>> + Send + Sync>;

/// The sources by their plugin's id.
static SOURCES: Mutex<Vec<(String, Source)>> = Mutex::new(Vec::new());

/// A source set under an id (None takes it away), the sources asked anew for it.
pub fn source(id: &str, f: Option<Source>) {
    if let Ok(mut all) = SOURCES.lock() {
        all.retain(|(i, _)| i != id);
        all.extend(f.map(|f| (id.to_string(), f)));
    }
    REFRESH.notify_one();
}

/// Every source's events in the span; one that fails said on stderr and left out, the others' kept.
async fn sourced() -> Vec<Value> {
    let all: Vec<(String, Source)> = SOURCES.lock().map(|s| s.clone()).unwrap_or_default();
    let (from, to) = span();
    let calls = all.iter().map(|(id, f)| {
        let call = f(iso(from), iso(to));
        async move { (id, call.await) }
    });
    let mut out = Vec::new();
    for (id, r) in futures_util::future::join_all(calls).await {
        match r {
            Ok(Value::Array(events)) => out.extend(events),
            Ok(Value::Null) => {}
            Ok(v) => eprintln!("ostrov: calendar: plugin {id}: not a list of events: {v}"),
            Err(e) => eprintln!("ostrov: calendar: plugin {id}: {e}"),
        }
    }
    out
}

/// The sources' events (null while there is none), sorted by their starts. An event needs a title and a start;
/// the rest has defaults (the end its start, not all day, no place, no colour), and what else it carries is
/// dropped.
fn merge(sources: usize, events: Vec<Value>) -> Value {
    if sources == 0 {
        return Value::Null;
    }
    let text = |e: &Value, k: &str| e[k].as_str().map(String::from);
    let mut all: Vec<Value> = events
        .iter()
        .filter_map(|e| {
            let (title, start) = (text(e, "title")?, text(e, "start")?);
            Some(json!({
                "title": title,
                "end": text(e, "end").unwrap_or(start.clone()),
                "start": start,
                "all_day": e["all_day"].as_bool().unwrap_or(false),
                "location": text(e, "location").unwrap_or_default(),
                "color": text(e, "color").unwrap_or_default(),
            }))
        })
        .collect();
    all.sort_by(|a, b| a["start"].as_str().cmp(&b["start"].as_str()));
    Value::Array(all)
}

/// The sources asked now and every 15 min or on a refresh, a kick each time.
pub async fn run(kick: Kick) {
    loop {
        let n = SOURCES.lock().map_or(0, |s| s.len());
        let events = merge(n, sourced().await);
        if let Ok(mut last) = LAST.lock() {
            *last = events;
        }
        let _ = kick.send(()).await;
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(15 * 60)) => {}
            () = REFRESH.notified() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugins_events_merged() {
        let events = vec![
            json!({"title": "C", "start": "2026-10-07T00:00:00", "end": "2026-10-08T00:00:00", "all_day": true}),
            json!({"title": "B", "start": "2026-10-06T09:00:00", "color": "#f00", "extra": 1}),
            json!({"title": "no start"}),
            json!({"start": "2026-10-06T09:00:00"}),
        ];
        let all = merge(2, events.clone());
        let titles: Vec<&str> = all.as_array().unwrap().iter().map(|e| e["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["B", "C"]);
        assert_eq!(all[0], json!({"title": "B", "start": "2026-10-06T09:00:00", "end": "2026-10-06T09:00:00",
            "all_day": false, "location": "", "color": "#f00"}));
        // no source: no calendar; sources without events: an empty one
        assert_eq!(merge(0, events), Value::Null);
        assert_eq!(merge(1, vec![]), json!([]));
    }
}
