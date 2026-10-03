//! The night light through Hyprland's screen shader (night.go): its config as the quick settings set it, kept in
//! ~/.cache/night-light.json, the mode (off, on, time: from to to, sun: sunset to sunrise where `location` puts
//! the machine) and the warmth in kelvin, 6500 neutral; a shader a warmth in ~/.cache/night-light.
//!
//! The local time here is the one Go's time package keeps: the zone of $TZ, or /etc/localtime, read from its
//! TZif file once, as Go loads time.Local once.
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{location, Res, USAGE};

const NS: i64 = 1_000_000_000;

/// The config, wmd's NightConfig: its defaults those of a file not there yet or missing fields.
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

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn night_file() -> String {
    format!("{}/.cache/night-light.json", home())
}

fn shader_dir() -> String {
    format!("{}/.cache/night-light", home())
}

fn load() -> Config {
    std::fs::read_to_string(night_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(c: &Config) -> Res {
    let b = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    std::fs::write(night_file(), b + "\n").map_err(|e| e.to_string())
}

/// The local zone: its transitions (unix seconds), the offset from each on, and the offset before the first.
type Zone = (Vec<i64>, Vec<i64>, i64);

/// The local zone as Go finds it: $TZ unset /etc/localtime, empty UTC, a name one of /usr/share/zoneinfo's
/// (":" before it allowed), or a file's path; UTC when it does not read.
fn zone() -> &'static Zone {
    static ZONE: OnceLock<Zone> = OnceLock::new();
    ZONE.get_or_init(|| {
        let path = match std::env::var("TZ") {
            Err(_) => Some("/etc/localtime".to_string()),
            Ok(tz) if tz.is_empty() => None,
            Ok(tz) if tz.starts_with('/') => Some(tz),
            Ok(tz) => Some(format!("/usr/share/zoneinfo/{}", tz.trim_start_matches(':'))),
        };
        path.and_then(|p| std::fs::read(p).ok()).and_then(|b| tzif(&b)).unwrap_or_default()
    })
}

/// A TZif file's transitions and offsets, from its 64-bit part where it has one (version 2 on). Past the last
/// transition its offset holds.
// ponytail: the footer's POSIX TZ rule is not read, so a zone with summer time is right only up to its last
// transition: 2037 in Ubuntu's "fat" files, sooner in "slim" ones; parse the footer if a system ships those.
fn tzif(b: &[u8]) -> Option<Zone> {
    let num = |at: usize, n: usize| -> Option<i64> {
        let v = b.get(at..at + n)?.iter().fold(0u64, |a, &x| a << 8 | u64::from(x));
        let shift = 64 - 8 * n as u32;
        Some(((v << shift) as i64) >> shift)
    };
    let counts = |at: usize| -> Option<[usize; 6]> {
        let mut c = [0; 6];
        for (i, v) in c.iter_mut().enumerate() {
            *v = num(at + 20 + 4 * i, 4)? as usize;
        }
        Some(c)
    };
    if b.get(..4)? != b"TZif" {
        return None;
    }
    let [isut, isstd, leap, timecnt, typecnt, charcnt] = counts(0)?;
    let (head, size) = if *b.get(4)? >= b'2' {
        (44 + timecnt * 5 + typecnt * 6 + charcnt + leap * 8 + isstd + isut, 8)
    } else {
        (0, 4)
    };
    let [_, _, _, timecnt, _, _] = counts(head)?;
    let body = head + 44;
    let types = body + timecnt * (size + 1);
    let utoff = |t: usize| num(types + 6 * t, 4);
    let mut times = Vec::with_capacity(timecnt);
    let mut offs = Vec::with_capacity(timecnt);
    for i in 0..timecnt {
        times.push(num(body + i * size, size)?);
        offs.push(utoff(usize::from(*b.get(body + timecnt * size + i)?))?);
    }
    Some((times, offs, utoff(0)?))
}

/// Unix seconds as local ones: the zone's offset then added.
pub fn local(secs: i64) -> i64 {
    let (times, offs, first) = zone();
    match times.partition_point(|&t| t <= secs) {
        0 => secs + first,
        i => secs + offs[i - 1],
    }
}

/// Now, in nanoseconds since the epoch.
pub fn now_ns() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64)
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
fn days(y: i64, m: i64, d: i64) -> i64 {
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
    let n = (day - days(civil(day).0, 1, 1) + 1) as f64;
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
    // minutes past the date's midnight in UTC, cut to whole nanoseconds as Go's time.Duration does
    let at = |minutes: f64| day * 86400 * NS + (minutes * 60e9) as i64;
    Some((at(720.0 - 4.0 * (lon + ha) - eqtime), at(720.0 - 4.0 * (lon - ha) - eqtime)))
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
        "sun" => location::load()
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
// wmd's night light at {k} K
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

/// A request to Hyprland's socket ("j/getoption ...", "keyword ..."), its answer; an error outside Hyprland,
/// which wm's own hyprctl does not tell.
fn hyprctl(req: &str) -> Result<String, String> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default();
    if sig.is_empty() {
        return Err("not under Hyprland".into());
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

/// Hyprland's screen shader set to what c wants now, touched only when that differs.
fn apply(c: &Config) -> Res {
    let want = if wanted(c, now_ns()) { shader(c.temp)? } else { String::new() };
    if current_shader() == want {
        return Ok(());
    }
    let want = if want.is_empty() { "[[EMPTY]]" } else { &want };
    hyprctl(&format!("keyword decoration:screen_shader {want}")).map(drop)
}

/// The screen as the config wants now: wmd watch's tick, every 3 s.
pub async fn apply_now() -> Res {
    apply(&load())
}

/// wmd's Night: whether the screen is warmed now, the config, and today's sunset and sunrise where a location
/// is known ("" where not, or the sun neither rises nor sets).
pub fn state() -> Value {
    let c = load();
    let l = location::load();
    let (mut sunrise, mut sunset) = (String::new(), String::new());
    let day = local(now_ns().div_euclid(NS)).div_euclid(86400);
    if let Some((rise, set)) = l.as_ref().and_then(|l| sun(day, l.lat, l.lon)) {
        sunrise = hhmm(local(rise.div_euclid(NS)));
        sunset = hhmm(local(set.div_euclid(NS)));
    }
    json!({
        "on": current_shader().starts_with(&shader_dir()),
        "mode": c.mode,
        "from": c.from,
        "to": c.to,
        "temp": c.temp,
        "sunset": sunset,
        "sunrise": sunrise,
        "located": l.is_some(),
    })
}

/// night mode off|on|time|sun, time FROM TO, temp K: each saved and applied; apply alone sets the screen as the
/// config wants now; preview K shows a warmth at once, saving nothing, until the next apply.
pub async fn cmd(args: &[&str]) -> Res {
    let mut c = load();
    match args {
        ["apply"] => return apply(&c),
        ["preview", k] => {
            // the screen at K now, whatever the mode, nothing saved: the warmth slider while it is dragged
            let k: i64 = k.parse().map_err(|_| USAGE.to_string())?;
            let path = shader(k.clamp(2500, 6500))?;
            return hyprctl(&format!("keyword decoration:screen_shader {path}")).map(drop);
        }
        ["mode", m @ ("off" | "on" | "time" | "sun")] => c.mode = m.to_string(),
        ["time", from, to] => {
            clock(from)?;
            clock(to)?;
            (c.from, c.to) = (from.to_string(), to.to_string());
        }
        ["temp", k] => c.temp = k.parse::<i64>().map_err(|_| USAGE.to_string())?.clamp(2500, 6500),
        _ => return Err(USAGE.into()),
    }
    save(&c)?;
    apply(&c)
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
        // the dates both ways, and the clock's bounds
        assert_eq!(civil(days(2026, 9, 23)), (2026, 9, 23));
        assert_eq!(clock("07:30"), Ok(450));
        assert_eq!(clock("24:00"), Err("\"24:00\" is not HH:MM".into()));
    }

    /// The state as it is here, to set beside wmd watch's: cargo test -- --ignored --nocapture night_state
    #[test]
    #[ignore]
    fn night_state() {
        println!("{}", state());
    }
}
