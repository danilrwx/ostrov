//! The player through MPRIS on the session bus (media.go).

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use zbus::zvariant::{self, OwnedValue};

use crate::services::{Ctx, Kick, Res};

const MPRIS: &str = "org.mpris.MediaPlayer2";
const PATH: &str = "/org/mpris/MediaPlayer2";

/// The MPRIS player playing (else the first there is): who it is, the track, its art, where it is in it
/// (microseconds, read when asked), and what it can do. Name empty without a player.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Media {
    name: String,
    identity: String,
    title: String,
    artist: String,
    art: String,
    playing: bool,
    position: i64,
    length: i64,
    can_prev: bool,
    can_next: bool,
    can_play: bool,
}

type Props = HashMap<String, OwnedValue>;

/// The players: the MPRIS names on the session bus, sorted.
async fn players(conn: &zbus::Connection) -> Vec<String> {
    let Ok(dbus) = zbus::fdo::DBusProxy::new(conn).await else { return Vec::new() };
    let Ok(names) = dbus.list_names().await else { return Vec::new() };
    let mut ps: Vec<String> =
        names.into_iter().map(|n| n.to_string()).filter(|n| n.starts_with(&format!("{MPRIS}."))).collect();
    ps.sort();
    ps
}

/// One interface's properties of a player's, none when it does not answer.
async fn get_all(conn: &zbus::Connection, name: &str, iface: &str) -> Props {
    let reply = conn
        .call_method(Some(name), PATH, Some("org.freedesktop.DBus.Properties"), "GetAll", &iface)
        .await;
    reply.ok().and_then(|r| r.body().deserialize().ok()).unwrap_or_default()
}

/// A property's value, a variant inside a variant (Metadata's entries) unwrapped.
fn prop<'a>(m: &'a Props, k: &str) -> Option<&'a zvariant::Value<'a>> {
    let mut v: &zvariant::Value = m.get(k)?;
    while let zvariant::Value::Value(inner) = v {
        v = inner;
    }
    Some(v)
}

fn text(m: &Props, k: &str) -> String {
    match prop(m, k) {
        Some(zvariant::Value::Str(s)) => s.to_string(),
        _ => String::new(),
    }
}

fn flag(m: &Props, k: &str) -> bool {
    matches!(prop(m, k), Some(zvariant::Value::Bool(true)))
}

async fn read_player(conn: &zbus::Connection, name: &str) -> Media {
    let root = get_all(conn, name, MPRIS).await;
    let p = get_all(conn, name, &format!("{MPRIS}.Player")).await;
    let meta = p.get("Metadata").and_then(|v| v.try_clone().ok()).and_then(|v| Props::try_from(v).ok());
    let meta = meta.unwrap_or_default();
    let artist = match prop(&meta, "xesam:artist") {
        Some(zvariant::Value::Array(a)) => {
            let names = a.iter().filter_map(|v| if let zvariant::Value::Str(s) = v { Some(s.as_str()) } else { None });
            names.collect::<Vec<_>>().join(", ")
        }
        _ => String::new(),
    };
    Media {
        name: name.into(),
        identity: text(&root, "Identity"),
        title: text(&meta, "xesam:title"),
        artist,
        art: text(&meta, "mpris:artUrl"),
        playing: text(&p, "PlaybackStatus") == "Playing",
        position: match prop(&p, "Position") {
            Some(zvariant::Value::I64(x)) => *x,
            _ => 0,
        },
        length: match prop(&meta, "mpris:length") {
            Some(zvariant::Value::I64(x)) => *x,
            Some(zvariant::Value::U64(x)) => *x as i64,
            _ => 0,
        },
        can_prev: flag(&p, "CanGoPrevious"),
        can_next: flag(&p, "CanGoNext"),
        can_play: flag(&p, "CanPlay"),
    }
}

/// The player playing, else the first; zero without a player.
async fn media_state(c: &Ctx) -> Media {
    let mut first = None;
    for n in players(&c.session).await {
        let m = read_player(&c.session, &n).await;
        if m.playing {
            return m;
        }
        first.get_or_insert(m);
    }
    first.unwrap_or_default()
}

pub async fn state(c: &Ctx) -> Value {
    serde_json::to_value(media_state(c).await).unwrap_or_default()
}

/// media play-pause|next|previous: the player state picks.
pub async fn cmd(c: &Ctx, args: &[&str]) -> Res {
    let method = match args {
        ["play-pause"] => "PlayPause",
        ["next"] => "Next",
        ["previous"] => "Previous",
        _ => return Err(super::MODULE.usage()),
    };
    let m = media_state(c).await;
    if m.name.is_empty() {
        return Ok(());
    }
    let iface = format!("{MPRIS}.Player");
    let reply = c.session.call_method(Some(m.name.as_str()), PATH, Some(iface.as_str()), method, &()).await;
    reply.map(drop).map_err(|e| e.to_string())
}

/// A kick on every change a player signals.
pub async fn events(c: Arc<Ctx>, kick: Kick) {
    let Ok(rule) = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).path(PATH).map(|b| b.build()) else {
        return;
    };
    let Ok(mut s) = zbus::MessageStream::for_match_rule(rule, &c.session, None).await else { return };
    while s.next().await.is_some() {
        if kick.send(()).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn media_state() {
        let (system, session) = (zbus::Connection::system().await.unwrap(), zbus::Connection::session().await.unwrap());
        let c = crate::services::Ctx { system, session };
        println!("media {}", super::state(&c).await);
    }
}
