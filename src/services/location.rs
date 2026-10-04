//! Where the machine is, for the weather and the night light's sunset and sunrise: set once by `location CITY`
//! (open-meteo's geocoding) or `location LAT LON`, kept in ~/.config/wmd.json (location.go).
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Res;

pub const USAGE: &str = "location CITY|LAT LON";

/// The place as it is named, and where it is.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Location {
    pub place: String,
    pub lat: f64,
    pub lon: f64,
}

fn file() -> String {
    format!("{}/.config/wmd.json", std::env::var("HOME").unwrap_or_default())
}

/// The location kept; None when there is none, it does not parse, or it is 0, 0 (never set).
pub fn load() -> Option<Location> {
    let l: Location = serde_json::from_str(&std::fs::read_to_string(file()).ok()?).ok()?;
    (l.lat != 0.0 || l.lon != 0.0).then_some(l)
}

/// The client for the internet (the weather, the geocoding): 10 s for a request, all of it, as wmd's was.
pub fn web() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().new_agent()
}

/// A GET of JSON, its answer as it came; blocking, so for spawn_blocking.
pub fn get_json(req: ureq::RequestBuilder<ureq::typestate::WithoutBody>) -> Result<Value, String> {
    let body = req.call().and_then(|mut r| r.body_mut().read_to_string()).map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

/// location CITY | LAT LON: the place saved, its name printed. Two words that are not both numbers are a
/// place's name too ("New York").
pub async fn cmd(args: &[&str]) -> Res {
    let coords = match args {
        [lat, lon] => lat.parse::<f64>().ok().zip(lon.parse::<f64>().ok()),
        _ => None,
    };
    let l = if args.is_empty() {
        return Err(USAGE.into());
    } else if let Some((lat, lon)) = coords {
        Location { place: format!("{lat:.2}, {lon:.2}"), lat, lon }
    } else {
        // a place by its name, the first open-meteo knows
        let name = args.join(" ");
        let q = name.clone();
        let found = tokio::task::spawn_blocking(move || {
            get_json(web().get("https://geocoding-api.open-meteo.com/v1/search?count=1").query("name", q))
        })
        .await
        .map_err(|e| e.to_string())??;
        let Some(r) = found["results"].get(0) else { return Err(format!("no place called {name}")) };
        let s = |k: &str| r[k].as_str().unwrap_or_default().to_string();
        let f = |k: &str| r[k].as_f64().unwrap_or(0.0);
        Location { place: format!("{}, {}", s("name"), s("country")), lat: f("latitude"), lon: f("longitude") }
    };
    let b = serde_json::to_string_pretty(&l).map_err(|e| e.to_string())?;
    std::fs::write(file(), b + "\n").map_err(|e| e.to_string())?;
    println!("{}", l.place);
    Ok(())
}
