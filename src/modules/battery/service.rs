//! The battery through UPower (battery.go).
use std::collections::HashMap;

use serde::{Serialize, Serializer};
use serde_json::Value;
use zbus::zvariant::OwnedValue;

use crate::services::Ctx;

const UPOWER: &str = "org.freedesktop.UPower";

/// UPower's display device: the charge in percent, its state (charging, discharging, pending-charge: plugged in
/// but held, fully-charged), the icon UPower names for it, and the seconds to empty or to full as it reckons them
/// (0 while it cannot tell); present false on a machine without one.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Battery {
    present: bool,
    #[serde(serialize_with = "go_float")]
    percent: f64,
    state: String,
    icon: String,
    to_empty: i64,
    to_full: i64,
}

/// A float as Go's encoding/json writes it: a whole one without its ".0" (70, not 70.0).
fn go_float<S: Serializer>(x: &f64, s: S) -> Result<S::Ok, S::Error> {
    if x.fract() == 0.0 && x.abs() < 1e15 { s.serialize_i64(*x as i64) } else { s.serialize_f64(*x) }
}

pub async fn state(c: &Ctx) -> Value {
    let b = match read(c).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("ostrov: battery: {}", crate::services::dbus::err(e));
            Battery::default()
        }
    };
    serde_json::to_value(b).unwrap_or_default()
}

async fn read(c: &Ctx) -> zbus::Result<Battery> {
    let reply = c
        .system
        .call_method(
            Some(UPOWER),
            "/org/freedesktop/UPower/devices/DisplayDevice",
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &"org.freedesktop.UPower.Device",
        )
        .await?;
    let props: HashMap<String, OwnedValue> = reply.body().deserialize()?;
    let state = prop(&props, "State").unwrap_or(0u32);
    Ok(Battery {
        present: prop(&props, "IsPresent").unwrap_or_default(),
        percent: prop(&props, "Percentage").unwrap_or_default(),
        state: match state {
            1 => "charging",
            2 => "discharging",
            3 => "empty",
            4 => "fully-charged",
            5 => "pending-charge",
            6 => "pending-discharge",
            _ => "",
        }
        .into(),
        icon: prop(&props, "IconName").unwrap_or_default(),
        to_empty: prop(&props, "TimeToEmpty").unwrap_or_default(),
        to_full: prop(&props, "TimeToFull").unwrap_or_default(),
    })
}

/// A property of the device's, None when it is missing or of another type.
fn prop<T: TryFrom<OwnedValue>>(props: &HashMap<String, OwnedValue>, k: &str) -> Option<T> {
    T::try_from(props.get(k)?.try_clone().ok()?).ok()
}
