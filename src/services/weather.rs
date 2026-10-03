//! The location's weather from open-meteo, no key needed (weather.go): now, the day's high and low, and the next
//! hours; icon names are Adwaita's (weather-clear-symbolic, ...).
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use super::location::{self, Location};
use super::night::{civil, hhmm, local, now_ns};
use super::Kick;

/// The last weather fetched, wmd's Weather (null before the first), refreshed every 15 min by run.
static LAST: Mutex<Value> = Mutex::new(Value::Null);

/// A WMO weather code as Adwaita's icon and a word for it, night icons for clear and few clouds.
fn wmo(code: i64, day: bool) -> (String, &'static str) {
    let n = if day { "" } else { "-night" };
    match code {
        0 => (format!("weather-clear{n}-symbolic"), "Clear"),
        c if c <= 2 => (format!("weather-few-clouds{n}-symbolic"), "Partly cloudy"),
        3 => ("weather-overcast-symbolic".into(), "Overcast"),
        c if c <= 48 => ("weather-fog-symbolic".into(), "Fog"),
        c if c <= 57 => ("weather-showers-scattered-symbolic".into(), "Drizzle"),
        c if c <= 67 || (80..=82).contains(&c) => ("weather-showers-symbolic".into(), "Rain"),
        c if c <= 77 || c == 85 || c == 86 => ("weather-snow-symbolic".into(), "Snow"),
        _ => ("weather-storm-symbolic".into(), "Thunderstorm"),
    }
}

/// Half away from zero, as wmd rounded.
fn round(f: f64) -> i64 {
    if f < 0.0 { (f - 0.5) as i64 } else { (f + 0.5) as i64 }
}

/// A float as Go's JSON writes it: a whole one without its ".0".
fn number(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 1e15 { json!(f as i64) } else { json!(f) }
}

/// The weather at l now; blocking, so for spawn_blocking. What is missing from the answer is 0, as Go's decoding
/// left it.
fn fetch(l: &Location) -> Result<Value, String> {
    let u = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}&timezone=auto&forecast_days=2\
         &current=temperature_2m,weather_code,is_day,wind_speed_10m&hourly=temperature_2m,weather_code,is_day\
         &daily=temperature_2m_max,temperature_2m_min&wind_speed_unit=ms",
        l.lat, l.lon
    );
    let r = location::get_json(location::web().get(&u))?;
    let (cur, hourly, daily) = (&r["current"], &r["hourly"], &r["daily"]);
    let f = |v: &Value| v.as_f64().unwrap_or(0.0);
    let list = |v: &Value| v.as_array().cloned().unwrap_or_default();
    let (icon, text) = wmo(cur["weather_code"].as_i64().unwrap_or(0), cur["is_day"].as_i64() == Some(1));
    let (max, min) = (list(&daily["temperature_2m_max"]), list(&daily["temperature_2m_min"]));
    let (high, low) = match (max.first(), min.first()) {
        (Some(h), Some(l)) => (round(f(h)), round(f(l))),
        _ => (0, 0),
    };
    // the next six hours, every other one, from the hour after now (both the place's time, open-meteo's
    // timezone=auto, and the machine's taken as one)
    let now = local(now_ns().div_euclid(1_000_000_000));
    let (y, m, d) = civil(now.div_euclid(86400));
    let hour = format!("{y:04}-{m:02}-{d:02}T{}", &hhmm(now)[..2]);
    let (temps, codes) = (list(&hourly["temperature_2m"]), list(&hourly["weather_code"]));
    let days = list(&hourly["is_day"]);
    let mut hours = Vec::new();
    for (i, t) in list(&hourly["time"]).iter().enumerate() {
        let t = t.as_str().unwrap_or_default();
        let (Some(h), Some(at)) = (t.get(..13), t.get(11..16)) else { continue };
        if h <= hour.as_str() || hours.len() == 6 || i >= temps.len() || i >= codes.len() || i % 2 == 1 {
            continue;
        }
        let (icon, _) = wmo(codes[i].as_i64().unwrap_or(0), days.get(i).and_then(Value::as_i64) == Some(1));
        hours.push(json!({"time": at, "temp": round(f(&temps[i])), "icon": icon}));
    }
    Ok(json!({
        "place": l.place,
        "temp": round(f(&cur["temperature_2m"])),
        "icon": icon,
        "text": text,
        "high": high,
        "low": low,
        "hours": hours,
        "at": hhmm(now),
        "wind": number(f(&cur["wind_speed_10m"])),
    }))
}

/// The last weather, null when none yet.
pub fn state() -> Value {
    LAST.lock().map(|w| w.clone()).unwrap_or_default()
}

/// The weather now and every 15 min (sooner, 1 min, after a failure), a kick each time there is a new one;
/// nothing fetched while no location is set.
pub async fn run(kick: Kick) {
    loop {
        let mut wait = Duration::from_secs(15 * 60);
        if let Some(l) = location::load() {
            match tokio::task::spawn_blocking(move || fetch(&l)).await {
                Ok(Ok(w)) => {
                    if let Ok(mut last) = LAST.lock() {
                        *last = w;
                    }
                    let _ = kick.send(()).await;
                }
                _ => wait = Duration::from_secs(60),
            }
        }
        tokio::time::sleep(wait).await;
    }
}

#[cfg(test)]
mod tests {
    /// The weather fetched once where the location says: cargo test -- --ignored --nocapture weather_once
    #[test]
    #[ignore]
    fn weather_once() {
        let l = super::location::load().expect("no location");
        println!("{}", super::fetch(&l).expect("fetch"));
    }
}
