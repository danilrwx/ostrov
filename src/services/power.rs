//! The power profile through power-profiles-daemon (power.go).
use serde::Serialize;
use serde_json::Value;
use zbus::zvariant::{self, OwnedValue};

use super::{Ctx, Res, USAGE};

/// power-profiles-daemon, by its old name, which every version still answers to.
const PPD: &str = "net.hadess.PowerProfiles";
const PATH: &str = "/net/hadess/PowerProfiles";
const PROPS: &str = "org.freedesktop.DBus.Properties";

/// The active power profile and the ones to pick from.
#[derive(Serialize, Default)]
struct Power {
    active: String,
    profiles: Vec<String>,
}

/// The profile now and the names of all of them; what was read before an error is kept, as wmd keeps it.
pub async fn state(c: &Ctx) -> Value {
    let mut p = Power::default();
    if let Err(e) = read(c, &mut p).await {
        eprintln!("ostrov: power: {}", dbus_err(e));
    }
    serde_json::to_value(p).unwrap_or_default()
}

async fn read(c: &Ctx, p: &mut Power) -> zbus::Result<()> {
    p.active = get(c, "ActiveProfile").await?.try_into()?;
    let profiles: Vec<std::collections::HashMap<String, OwnedValue>> = get(c, "Profiles").await?.try_into()?;
    p.profiles = profiles.iter().filter_map(|pr| pr.get("Profile")?.downcast_ref::<String>().ok()).collect();
    Ok(())
}

async fn get(c: &Ctx, prop: &str) -> zbus::Result<OwnedValue> {
    let reply = c.system.call_method(Some(PPD), PATH, Some(PROPS), "Get", &(PPD, prop)).await?;
    Ok(reply.body().deserialize()?)
}

/// power set PROFILE.
pub async fn cmd(c: &Ctx, args: &[&str]) -> Res {
    let ["set", profile] = args else { return Err(USAGE.into()) };
    let body = (PPD, "ActiveProfile", zvariant::Value::from(*profile));
    c.system.call_method(Some(PPD), PATH, Some(PROPS), "Set", &body).await.map(drop).map_err(dbus_err)
}

/// A D-Bus error as wmd's godbus says it: the error's message, or its name when it came without one.
pub(super) fn dbus_err(e: zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, desc, _) => desc.unwrap_or_else(|| name.to_string()),
        e => e.to_string(),
    }
}
