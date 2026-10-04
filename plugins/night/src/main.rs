//! ostrov's night light through Hyprland's screen shader: its config as its menu and commands set it, kept in
//! ~/.local/state/ostrov/night.json (where the built-in one kept it, so it carries over), the mode (off, on, time:
//! from to to, sun: sunset to sunrise where `ostrov location` puts the machine, read from its
//! ~/.local/state/ostrov/location.json) and the warmth in kelvin, 6500 neutral; a shader a warmth in
//! ~/.cache/ostrov/night-light. The screen is set to what the config wants every 3 s, so a Hyprland reload that
//! drops the shader gets it back.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{SystemTime, UNIX_EPOCH};

use ostrov_plugin::{Host, Plugin, Value, json, t};
use serde::{Deserialize, Serialize};

const NS: i64 = 1_000_000_000;
/// The timer keeping the screen to the config.
const TICK: u32 = 1;
const ICON: &str = "night-light-symbolic";
const USAGE: &str = "usage: ostrov plugin night on|off|toggle | mode off|on|time|sun | time FROM TO | warmth K";

/// The config: its defaults those of a file not there yet or missing fields.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    mode: String,
    from: String,
    to: String,
    temp: i64,
}

impl Default for Config {
    fn default() -> Self {
        Config { mode: "off".into(), from: "21:00".into(), to: "07:00".into(), temp: 4000 }
    }
}

/// Where the machine is, as ostrov's `location` keeps it.
#[derive(Deserialize)]
struct Location {
    lat: f64,
    lon: f64,
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn night_file() -> String {
    format!("{}/.local/state/ostrov/night.json", home())
}

fn shader_dir() -> String {
    format!("{}/.cache/ostrov/night-light", home())
}

fn load() -> Config {
    std::fs::read_to_string(night_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(c: &Config) -> Result<(), String> {
    let b = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    if let Some(dir) = std::path::Path::new(&night_file()).parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(night_file(), b + "\n").map_err(|e| e.to_string())
}

/// The location kept; None when there is none, it does not parse, or it is 0, 0 (never set).
fn location() -> Option<Location> {
    let path = format!("{}/.local/state/ostrov/location.json", home());
    let l: Location = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    (l.lat != 0.0 || l.lon != 0.0).then_some(l)
}

/// Unix seconds as local ones: the local zone's offset then added, its summer time and all.
fn local(secs: i64) -> i64 {
    let t = secs as libc::time_t;
    // SAFETY: localtime_r fills the tm handed to it and touches nothing else of ours
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return secs;
    }
    secs + tm.tm_gmtoff
}

/// Now, in nanoseconds since the epoch.
fn now_ns() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64)
}

/// The year of a day since the epoch (Howard Hinnant's civil_from_days).
fn year(days: i64) -> i64 {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    yoe + era * 400 + i64::from(mp >= 10)
}

/// The day since the epoch of a date (Hinnant's days_from_civil).
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

/// Local seconds as "HH:MM".
fn hhmm(local: i64) -> String {
    format!("{:02}:{:02}", local.rem_euclid(86400) / 3600, local.rem_euclid(3600) / 60)
}

/// "HH:MM" as minutes past midnight.
fn clock(s: &str) -> Result<i64, String> {
    let (h, m) = s.split_once(':').unwrap_or((s, ""));
    match (h.parse::<i64>(), m.parse::<i64>()) {
        (Ok(hh), Ok(mm)) if (0..=23).contains(&hh) && (0..=59).contains(&mm) => Ok(hh * 60 + mm),
        _ => Err(format!("{s:?} is not HH:MM")),
    }
}

/// Whether minute now falls in [from, to), the span running past midnight when to < from.
fn within(now: i64, from: i64, to: i64) -> bool {
    if from <= to { now >= from && now < to } else { now >= from || now < to }
}

/// The sunrise and sunset of a local date (days since the epoch) at lat, lon, in nanoseconds since the epoch,
/// by NOAA's simplified equations (a minute or two off, enough for a screen's warmth); None in a polar day or
/// night.
fn sun(day: i64, lat: f64, lon: f64) -> Option<(i64, i64)> {
    use std::f64::consts::PI;
    let rad = PI / 180.0;
    let n = (day - days(year(day), 1, 1) + 1) as f64;
    let g = 2.0 * PI / 365.0 * (n - 1.0);
    let eqtime = 229.18
        * (0.000075 + 0.001868 * g.cos() - 0.032077 * g.sin() - 0.014615 * (2.0 * g).cos()
            - 0.040849 * (2.0 * g).sin());
    let decl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos()
        + 0.000907 * (2.0 * g).sin()
        - 0.002697 * (3.0 * g).cos()
        + 0.00148 * (3.0 * g).sin();
    let cos_h = (90.833 * rad).cos() / ((lat * rad).cos() * decl.cos()) - (lat * rad).tan() * decl.tan();
    if !(-1.0..=1.0).contains(&cos_h) {
        return None;
    }
    let ha = cos_h.acos() / rad;
    // minutes past the date's midnight in UTC, cut to whole nanoseconds
    let at = |minutes: f64| day * 86400 * NS + (minutes * 60e9) as i64;
    Some((at(720.0 - 4.0 * (lon + ha) - eqtime), at(720.0 - 4.0 * (lon - ha) - eqtime)))
}

/// Today's sunset and sunrise as "HH:MM", None without a location or where the sun neither rises nor sets.
fn sun_times() -> Option<(String, String)> {
    let l = location()?;
    let (rise, set) = sun(local(now_ns().div_euclid(NS)).div_euclid(86400), l.lat, l.lon)?;
    Some((hhmm(local(set.div_euclid(NS))), hhmm(local(rise.div_euclid(NS)))))
}

/// Whether the screen should be warm at now (nanoseconds since the epoch) under c.
fn wanted(c: &Config, now: i64) -> bool {
    let local = local(now.div_euclid(NS));
    match c.mode.as_str() {
        "on" => true,
        "time" => match (clock(&c.from), clock(&c.to)) {
            (Ok(from), Ok(to)) => within(local.rem_euclid(86400) / 60, from, to),
            _ => false,
        },
        "sun" => location()
            .and_then(|l| sun(local.div_euclid(86400), l.lat, l.lon))
            .is_some_and(|(rise, set)| now > set || now < rise),
        _ => false,
    }
}

/// The red, green and blue factors of a white point at k kelvin (Tanner Helland's fit of the black body), 6500
/// giving no change.
fn gains(k: i64) -> (f64, f64, f64) {
    let t = k.clamp(1000, 6500) as f64 / 100.0;
    let (r, g, b);
    if t > 66.0 {
        (r, g, b) = (1.2929 * (t - 60.0).powf(-0.1332), 1.1298 * (t - 60.0).powf(-0.0755), 1.0);
    } else {
        r = 1.0;
        g = (99.4708 * t.ln() - 161.1196) / 255.0;
        b = if t <= 19.0 { 0.0 } else { (138.5177 * (t - 10.0).ln() - 305.0448) / 255.0 };
    }
    let clamp = |v: f64| v.clamp(0.0, 1.0);
    (clamp(r), clamp(g), clamp(b))
}

/// The warm screen shader for k kelvin written, its path: one file a warmth, so Hyprland takes a new one afresh.
fn shader(k: i64) -> Result<String, String> {
    std::fs::create_dir_all(shader_dir()).map_err(|e| e.to_string())?;
    let path = format!("{}/night-{k}.frag", shader_dir());
    let (r, g, b) = gains(k);
    let src = format!(
        "#version 300 es
// ostrov's night light at {k} K
precision highp float;
in vec2 v_texcoord;
uniform sampler2D tex;
out vec4 fragColor;
void main() {{
    vec4 c = texture(tex, v_texcoord);
    fragColor = vec4(c.r * {r:.4}, c.g * {g:.4}, c.b * {b:.4}, c.a);
}}
"
    );
    std::fs::write(&path, src).map_err(|e| e.to_string())?;
    Ok(path)
}

/// A request to Hyprland's socket ("j/getoption ...", "keyword ..."), as hyprctl sends it, its answer; outside
/// Hyprland an error naming the request.
fn hyprctl(req: &str) -> Result<String, String> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default();
    if sig.is_empty() {
        return Err(format!("not under Hyprland: hyprctl {req}"));
    }
    let run = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
    let mut s = UnixStream::connect(format!("{run}/hypr/{sig}/.socket.sock")).map_err(|e| e.to_string())?;
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut out = String::new();
    s.read_to_string(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Hyprland's screen shader now, "" for none.
fn current_shader() -> String {
    let Ok(out) = hyprctl("j/getoption decoration:screen_shader") else { return String::new() };
    let opt: Value = serde_json::from_str(&out).unwrap_or_default();
    match opt["str"].as_str() {
        Some("[[EMPTY]]") | None => String::new(),
        Some(s) => s.to_string(),
    }
}

/// Hyprland's screen shader set to path ("" for none), touched only when that differs.
fn set_shader(path: &str) -> Result<(), String> {
    if current_shader() == path {
        return Ok(());
    }
    let path = if path.is_empty() { "[[EMPTY]]" } else { path };
    hyprctl(&format!("keyword decoration:screen_shader {path}")).map(drop)
}

struct Night {
    /// whether the screen is wanted warm, as last applied
    on: bool,
    /// the last apply's error, logged once
    failed: String,
    /// what the menu's hours were last refused for
    error: String,
    ticking: bool,
}

impl Night {
    /// The screen set to what the config wants now, the widget kicked when that changed.
    fn apply(&mut self, host: &Host) {
        let c = load();
        let on = wanted(&c, now_ns());
        let r = if on { shader(c.temp) } else { Ok(String::new()) }.and_then(|p| set_shader(&p));
        let failed = r.err().unwrap_or_default();
        if failed != self.failed && !failed.is_empty() {
            host.log(&failed);
        }
        self.failed = failed;
        if on != self.on {
            self.on = on;
            host.kick();
        }
    }

    /// One command's change of the config, saved and applied.
    fn change(&mut self, host: &Host, f: impl FnOnce(&mut Config) -> Result<(), String>) -> Result<(), String> {
        let mut c = load();
        f(&mut c)?;
        save(&c)?;
        self.apply(host);
        host.kick();
        Ok(())
    }
}

impl Plugin for Night {
    fn on_config(&mut self, host: &Host, _: &Value) {
        if !self.ticking {
            self.ticking = true;
            self.on_timer(host, TICK);
        }
    }

    fn on_timer(&mut self, host: &Host, id: u32) {
        if id == TICK {
            self.apply(host);
            host.set_timer(3000, TICK);
        }
    }

    /// Whether the screen is warm now, the config, and today's sunset and sunrise ("" without a location).
    fn state(&mut self, _: &Host) -> Value {
        let c = load();
        let (sunset, sunrise) = sun_times().unwrap_or_default();
        json!({"on": self.on, "mode": c.mode, "from": c.from, "to": c.to, "temp": c.temp,
               "sunset": sunset, "sunrise": sunrise, "located": location().is_some()})
    }

    fn run(&mut self, host: &Host, args: &[&str], _: Option<&str>) -> Result<String, String> {
        let on = self.on;
        match args {
            ["on"] => self.change(host, |c| Ok(c.mode = "on".into()))?,
            ["off"] => self.change(host, |c| Ok(c.mode = "off".into()))?,
            ["toggle"] => self.change(host, |c| Ok(c.mode = if on { "off" } else { "on" }.into()))?,
            ["mode", m @ ("off" | "on" | "time" | "sun")] => self.change(host, |c| Ok(c.mode = m.to_string()))?,
            ["time", from, to] => self.change(host, |c| {
                clock(from)?;
                clock(to)?;
                (c.from, c.to) = (from.to_string(), to.to_string());
                Ok(())
            })?,
            ["warmth", k] => {
                let k = k.parse::<i64>().map_err(|_| USAGE.to_string())?.clamp(2500, 6500);
                self.change(host, |c| Ok(c.temp = k))?
            }
            _ => return Err(USAGE.into()),
        }
        Ok(if self.on { "on" } else { "off" }.into())
    }

    fn render(&mut self, _: &Host, widget: &str) -> Option<Value> {
        if widget == "toggle#bar" {
            return Some(json!({"type": "image", "icon": ICON, "active": self.on}));
        }
        let c = load();
        let hours = format!("{} – {}", c.from, c.to);
        let sun = sun_times().map_or_else(|| t("no location").to_string(), |(set, rise)| format!("{set} – {rise}"));
        let sub = match c.mode.as_str() {
            "on" => format!("{} K", c.temp),
            "time" => hours.clone(),
            "sun" => sun.clone(),
            _ => t("Off").into(),
        };
        let modes = [("off", t("Off"), ""), ("on", t("Always on"), ""), ("time", t("Scheduled"), &*hours),
            ("sun", t("Sunset to sunrise"), &*sun)];
        let mut menu: Vec<Value> = modes
            .iter()
            .map(|(m, text, note)| json!({"type": "row", "id": format!("mode-{m}"), "text": text, "note": note,
                "on": c.mode == *m}))
            .collect();
        if c.mode == "time" {
            menu.push(json!({"type": "box", "orientation": "horizontal", "children": [
                {"type": "label", "class": "dim", "text": t("from")},
                {"type": "entry", "id": "from", "placeholder": c.from},
                {"type": "label", "class": "dim", "text": t("to")},
                {"type": "entry", "id": "to", "placeholder": c.to},
            ]}));
        }
        if !self.error.is_empty() {
            menu.push(json!({"type": "label", "class": "error", "text": self.error}));
        }
        menu.push(json!({"type": "box", "orientation": "horizontal", "children": [
            {"type": "label", "class": "dim", "text": t("Warmth")},
            {"type": "label", "class": "dim", "text": format!("{} K", c.temp)},
        ]}));
        menu.push(json!({"type": "slider", "id": "warmth", "icon": ICON, "value": (6500 - c.temp) as f64 / 4000.0}));
        Some(json!({"type": "toggle", "id": "night", "icon": ICON, "title": t("Night Light"), "sub": sub,
            "on": self.on, "menu": {"type": "box", "children": menu}}))
    }

    fn on_event(&mut self, host: &Host, _: &str, node: &str, _: &str, value: &str) {
        let r = match node {
            "night" => {
                let m = if value == "true" { "on" } else { "off" };
                self.change(host, |c| Ok(c.mode = m.into()))
            }
            "from" | "to" => self.change(host, |c| {
                clock(value)?;
                *if node == "from" { &mut c.from } else { &mut c.to } = value.into();
                Ok(())
            }),
            "warmth" => {
                // shown on the screen at once whatever the mode while it is dragged, the next tick putting the
                // screen back to the mode's
                let k = (6500.0 - value.parse::<f64>().unwrap_or(0.4) * 4000.0).round() as i64;
                let r = self.change(host, |c| Ok(c.temp = k.clamp(2500, 6500)));
                if let Err(e) = shader(k.clamp(2500, 6500)).and_then(|p| set_shader(&p)) {
                    host.log(e);
                }
                r
            }
            m => match m.strip_prefix("mode-") {
                Some(m) => self.change(host, |c| Ok(c.mode = m.into())),
                None => return,
            },
        };
        self.error = r.err().unwrap_or_default();
        host.kick();
    }
}

fn main() {
    ostrov_plugin::run(Night { on: false, failed: String::new(), error: String::new(), ticking: false });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night() {
        // a span past midnight
        assert!(within(23 * 60, 21 * 60, 7 * 60) && within(6 * 60, 21 * 60, 7 * 60));
        assert!(!within(12 * 60, 21 * 60, 7 * 60), "within: 21:00-07:00 wrong");
        // Riga, the equinox: sunrise and sunset about 12 h apart, around 06:20 and 18:30 UTC
        let (rise, set) = sun(days(2026, 9, 23), 56.95, 24.1).expect("sun: none");
        let hour = |ns: i64| ns.div_euclid(NS).rem_euclid(86400) / 3600;
        assert!((3..=5).contains(&hour(rise)) && (15..=17).contains(&hour(set)), "sun: rise {rise} set {set}");
        // 6500 K no change, warmer less blue
        let (r, g, b) = gains(6500);
        assert!(r >= 0.99 && g >= 0.97 && b >= 0.97, "gains(6500) = {r} {g} {b}");
        assert!(gains(3000).2 <= 0.7, "gains(3000) blue {}", gains(3000).2);
        // the year of a day, January's and December's, and the clock's bounds
        assert_eq!((year(days(2026, 1, 1)), year(days(2026, 12, 31)), year(days(2027, 1, 1))), (2026, 2026, 2027));
        assert_eq!(clock("07:30"), Ok(450));
        assert_eq!(clock("24:00"), Err("\"24:00\" is not HH:MM".into()));
        assert_eq!(hhmm(7 * 3600 + 5 * 60), "07:05");
    }
}
