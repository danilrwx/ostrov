//! The status line for dwl's bar, read from dwl's stdin a line at a time: what is worth a look only when it is (the
//! mic, camera or screen taken, the sound muted, no network, notifications waiting or held back), the battery and
//! the clock always. Each block starts with ^bl(NAME), the click's target for the fork's bar, colour by
//! ^fg(RRGGBB) ... ^fg() (README of ~/w/dwl).

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gio::glib;
use serde_json::Value;
use zbus::zvariant::OwnedValue;

use crate::services::dbus::call;
use crate::services::{signals, Ctx, Kick};
use crate::{audio, battery};

/// mako, by the name it holds on the session bus and its own interface's object.
pub const MAKO: &str = "org.freedesktop.Notifications";
const MAKO_PATH: &str = "/fr/emersion/Mako";

const RED: &str = "ff5555";

/// What the line shows.
#[derive(Default)]
pub struct Status {
    pub privacy: bool,
    pub muted: bool,
    pub offline: bool,
    /// mako's notifications on screen and whether in do-not-disturb; None without mako
    pub notes: Option<(usize, bool)>,
    /// the charge in percent and whether charging; None without a battery
    pub battery: Option<(f64, bool)>,
    pub clock: String,
}

pub fn line(s: &Status) -> String {
    let mut blocks = Vec::new();
    if s.privacy {
        blocks.push(("privacy", red("●")));
    }
    if s.muted {
        blocks.push(("mute", "🔇".into()));
    }
    if s.offline {
        blocks.push(("net", "✗".into()));
    }
    if let Some((n, dnd)) = s.notes {
        let text = format!("{}{}", if dnd { "🔕" } else { "" }, if n > 0 { format!("✉{n}") } else { String::new() });
        if !text.is_empty() {
            blocks.push(("notes", text));
        }
    }
    if let Some((p, charging)) = s.battery {
        let text = format!("{p:.0}%{}", if charging { "⚡" } else { "" });
        blocks.push(("battery", if p < 15.0 { red(&text) } else { text }));
    }
    blocks.push(("clock", s.clock.clone()));
    blocks.iter().map(|(name, text)| format!("^bl({name}){text}")).collect::<Vec<_>>().join(" ")
}

fn red(text: &str) -> String {
    format!("^fg({RED}){text}^fg()")
}

/// The clock as the bar shows it, "Mon 05 Oct 09:22", the names in ostrov's language.
fn clock() -> String {
    let now = glib::DateTime::now_local();
    now.map(|now| crate::services::time::format_time(&now, "%a %d %b %H:%M")).unwrap_or_default()
}

/// Whether a default route is there, IPv4's or IPv6's (not the kernel's unreachable one on lo): online by
/// whatever way, Wi-Fi, a cable or a tunnel.
fn online(v4: &str, v6: &str) -> bool {
    let v4 = v4.lines().skip(1).any(|l| l.split_whitespace().nth(1) == Some("00000000"));
    let v6 = v6.lines().any(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        f.len() == 10 && f[0].bytes().all(|b| b == b'0') && f[1] == "00" && f[9] != "lo"
    });
    v4 || v6
}

/// mako's notifications on screen and whether do-not-disturb is among its modes; None while mako is not the one
/// holding the name (ostrov is, under Hyprland).
pub async fn notes(c: &Ctx) -> Option<(usize, bool)> {
    let list: Vec<HashMap<String, OwnedValue>> =
        call(&c.session, MAKO, MAKO_PATH, "fr.emersion.Mako.ListNotifications", &()).await.ok()?;
    let modes: Vec<String> =
        call(&c.session, MAKO, MAKO_PATH, "fr.emersion.Mako.ListModes", &()).await.unwrap_or_default();
    Some((list.len(), modes.iter().any(|m| m == "do-not-disturb")))
}

/// Whether the sound's state says the mic, a camera or the screen is taken.
pub fn privacy(audio: &Value) -> bool {
    let some = |k: &str| audio[k].as_array().is_some_and(|a| !a.is_empty());
    some("micApps") || some("camApps") || audio["screen"] == true
}

pub async fn read(c: &Ctx) -> Status {
    let (audio, battery, notes) = tokio::join!(audio::service::state(), battery::service::state(c), notes(c));
    let proc = |f: &str| std::fs::read_to_string(f).unwrap_or_default();
    let charging = matches!(battery["state"].as_str(), Some("charging" | "pending-charge"));
    Status {
        privacy: privacy(&audio),
        muted: audio["muted"] == true,
        offline: !online(&proc("/proc/net/route"), &proc("/proc/net/ipv6_route")),
        notes,
        battery: (battery["present"] == true).then(|| (battery["percent"].as_f64().unwrap_or(0.0), charging)),
        clock: clock(),
    }
}

/// A kick on what changes mako's list or modes: a notification sent or closed, a mode set, dismissed, through a
/// monitor of the session bus (mako signals no mode's change); the List* and Get* calls, ostrov-ctl's own among
/// them, left out.
async fn mako(kick: Kick) {
    use futures_util::StreamExt;
    let watch = async {
        let conn = zbus::connection::Builder::session()?.build().await?;
        let rules = [
            zbus::MatchRule::builder().interface(MAKO)?.build(),
            zbus::MatchRule::builder().interface("fr.emersion.Mako")?.build(),
        ];
        zbus::fdo::MonitoringProxy::new(&conn).await?.become_monitor(&rules, 0).await?;
        let mut s = zbus::MessageStream::from(conn);
        while let Some(Ok(m)) = s.next().await {
            let member = m.header().member().map(|m| m.to_string()).unwrap_or_default();
            if !member.starts_with("List") && !member.starts_with("Get") {
                let _ = kick.send(()).await;
            }
        }
        zbus::Result::Ok(())
    };
    if let Err(e) = watch.await {
        eprintln!("ostrov-ctl: notifications: {e}");
    }
}

/// The time to the next minute.
fn to_minute() -> Duration {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    Duration::from_millis(60_000 - ms % 60_000)
}

/// The line, then again on every change: PipeWire's (mute, the apps recording), UPower's, iwd's and
/// NetworkManager's signals, mako's, 100 ms after the last of a burst; and at each minute for the clock (and the
/// routes, which nothing above tells of a cable's). Only a line that differs goes out; dwl gone, it ends (its
/// pw-dump --monitor at its next write to the pipe left).
pub async fn watch(c: Arc<Ctx>) {
    let (kick, kicks) = async_channel::unbounded::<()>();
    tokio::spawn(audio::service::events(kick.clone()));
    for sender in ["org.freedesktop.UPower", "net.connman.iwd", "org.freedesktop.NetworkManager"] {
        tokio::spawn(signals(c.clone(), sender, kick.clone()));
    }
    tokio::spawn(mako(kick));
    let mut last = String::new();
    loop {
        let l = line(&read(&c).await);
        if l != last {
            let mut out = std::io::stdout().lock();
            if writeln!(out, "{l}").and_then(|()| out.flush()).is_err() {
                return;
            }
            last = l;
        }
        tokio::select! {
            Ok(()) = kicks.recv() => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                while kicks.try_recv().is_ok() {}
            }
            () = tokio::time::sleep(to_minute()) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_is_worth_a_look() {
        let clock = "Mon 05 Oct 09:22".into();
        let s = Status { battery: Some((76.4, false)), clock, notes: Some((0, false)), ..Default::default() };
        assert_eq!(line(&s), "^bl(battery)76% ^bl(clock)Mon 05 Oct 09:22");
        let s = Status { battery: None, notes: None, clock: "09:22".into(), ..Default::default() };
        assert_eq!(line(&s), "^bl(clock)09:22");
    }

    #[test]
    fn every_block() {
        let s = Status {
            privacy: true,
            muted: true,
            offline: true,
            notes: Some((3, true)),
            battery: Some((9.0, true)),
            clock: "09:22".into(),
        };
        assert_eq!(
            line(&s),
            "^bl(privacy)^fg(ff5555)●^fg() ^bl(mute)🔇 ^bl(net)✗ ^bl(notes)🔕✉3 \
             ^bl(battery)^fg(ff5555)9%⚡^fg() ^bl(clock)09:22"
        );
        let s = Status { notes: Some((0, true)), clock: "x".into(), ..Default::default() };
        assert_eq!(line(&s), "^bl(notes)🔕 ^bl(clock)x");
    }

    #[test]
    fn default_routes() {
        let v4 = "Iface\tDestination\tGateway\nwlan0\t00000000\t0101A8C0\nwlan0\t0001A8C0\t00000000\n";
        let v4_none = "Iface\tDestination\tGateway\nwlan0\t0001A8C0\t00000000\n";
        let zeros = "00000000000000000000000000000000";
        let v6_lo = format!("{zeros} 00 {zeros} 00 {zeros} ffffffff 00000001 00000000 00200200 lo\n");
        let gw = "fe800000000000000000000000000001";
        let v6 = format!("{zeros} 00 {zeros} 00 {gw} 00000400 00000001 00000000 00000003 wlan0\n");
        assert!(online(v4, ""));
        assert!(!online(v4_none, &v6_lo));
        assert!(online(v4_none, &v6));
    }

    #[test]
    fn privacy_of_the_sound() {
        assert!(!privacy(&serde_json::json!({"micApps": [], "camApps": [], "screen": false})));
        assert!(privacy(&serde_json::json!({"micApps": [{"id": 1, "name": "x"}]})));
        assert!(privacy(&serde_json::json!({"camApps": ["zoom"]})));
        assert!(privacy(&serde_json::json!({"screen": true})));
    }
}
