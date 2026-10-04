//! Bluetooth through BlueZ.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use zbus::zvariant::ObjectPath;

use crate::services::rfkill;
use crate::services::dbus::{call, err, managed, prop, set_property};
use crate::services::{Ctx, Res};

const BLUEZ: &str = "org.bluez";

/// The pairing agent's object path on the system bus, ostrov's own.
const AGENT_PATH: &str = "/dev/ostrov/bt_agent";

/// A Bluetooth device paired, or found by a scan; icon is BlueZ's (audio-headset, input-mouse, ...).
#[derive(Serialize)]
struct Device {
    address: String,
    name: String,
    icon: String,
    paired: bool,
    connected: bool,
    /// its charge in percent, if it tells it (BlueZ's Battery1: a headset's, a mouse's)
    #[serde(skip_serializing_if = "Option::is_none")]
    battery: Option<u8>,
}

/// The radio's state, the first device connected, whether a scan runs, the paired devices by name, then those a
/// scan found that have a name.
#[derive(Serialize, Default)]
struct Bluetooth {
    on: bool,
    connected: String,
    discovering: bool,
    devices: Vec<Device>,
}

pub async fn state(c: &Ctx) -> Value {
    let b = bluetooth(c).await.unwrap_or_else(|e| {
        eprintln!("ostrov: bt: {e}");
        Bluetooth::default()
    });
    serde_json::to_value(b).unwrap_or_default()
}

async fn bluetooth(c: &Ctx) -> Result<Bluetooth, String> {
    let mut b = Bluetooth::default();
    for ifaces in managed(&c.system, BLUEZ).await?.values() {
        if let Some(a) = ifaces.get("org.bluez.Adapter1") {
            b.on |= prop(Some(a), "Powered").unwrap_or(false);
            b.discovering |= prop(Some(a), "Discovering").unwrap_or(false);
        }
        let Some(d) = ifaces.get("org.bluez.Device1") else { continue };
        let dev = Device {
            address: prop(Some(d), "Address").unwrap_or_default(),
            name: prop(Some(d), "Alias").unwrap_or_default(),
            icon: prop(Some(d), "Icon").unwrap_or_default(),
            paired: prop(Some(d), "Paired").unwrap_or_default(),
            connected: prop(Some(d), "Connected").unwrap_or_default(),
            // 0 as good as untold: a headset's empty GATT battery service says it however charged it is
            battery: prop::<u8>(ifaces.get("org.bluez.Battery1"), "Percentage").filter(|p| *p > 0),
        };
        // a device found but unnamed is but its address: left out
        if !dev.paired && !d.contains_key("Name") {
            continue;
        }
        b.devices.push(dev);
    }
    b.devices.sort_by(|x, y| y.paired.cmp(&x.paired).then_with(|| x.name.cmp(&y.name)));
    if let Some(d) = b.devices.iter().find(|d| d.connected) {
        b.connected.clone_from(&d.name);
    }
    Ok(b)
}

/// The path of a device by its address.
async fn device(c: &Ctx, address: &str) -> Result<String, String> {
    let objs = managed(&c.system, BLUEZ).await?;
    objs.iter()
        .find(|(_, ifaces)| prop::<&str>(ifaces.get("org.bluez.Device1"), "Address") == Some(address))
        .map(|(p, _)| p.to_string())
        .ok_or_else(|| format!("no device {address}"))
}

/// The paths of the adapters.
async fn adapters(c: &Ctx) -> Result<Vec<String>, String> {
    let objs = managed(&c.system, BLUEZ).await?;
    Ok(objs
        .into_iter()
        .filter(|(_, ifaces)| ifaces.contains_key("org.bluez.Adapter1"))
        .map(|(p, _)| p.to_string())
        .collect())
}

async fn adapter(c: &Ctx) -> Result<String, String> {
    adapters(c).await?.into_iter().next().ok_or_else(|| "no Bluetooth adapter".into())
}

pub async fn cmd(c: &Ctx, args: &[&str]) -> Res {
    match args {
        ["on"] => {
            // unblocked, the adapter comes up unpowered: powered on as well
            rfkill::set_blocked(rfkill::BLUETOOTH, false)?;
            for p in adapters(c).await? {
                set_property(&c.system, BLUEZ, &p, "org.bluez.Adapter1.Powered", true).await?;
            }
            Ok(())
        }
        ["off"] => rfkill::set_blocked(rfkill::BLUETOOTH, true),
        [w @ ("connect" | "disconnect"), address] => {
            let path = device(c, address).await?;
            let method = if *w == "connect" { "Connect" } else { "Disconnect" };
            call(&c.system, BLUEZ, &path, &format!("org.bluez.Device1.{method}"), &()).await
        }
        ["scan"] => scan(c).await,
        ["pair", address] => pair(c, address).await,
        ["forget", address] => {
            let path = device(c, address).await?;
            let adapter = adapter(c).await?;
            let path = ObjectPath::try_from(path.as_str()).map_err(|e| e.to_string())?;
            call(&c.system, BLUEZ, &adapter, "org.bluez.Adapter1.RemoveDevice", &(path,)).await
        }
        _ => Err(super::MODULE.usage()),
    }
}

/// Looks for devices for 20 s: BlueZ keeps a discovery only while the client that started it stays on the bus,
/// so this waits it out; what it finds reaches the watcher through BlueZ's signals.
async fn scan(c: &Ctx) -> Res {
    let adapter = adapter(c).await?;
    call::<_, ()>(&c.system, BLUEZ, &adapter, "org.bluez.Adapter1.StartDiscovery", &()).await?;
    tokio::time::sleep(Duration::from_secs(20)).await;
    call(&c.system, BLUEZ, &adapter, "org.bluez.Adapter1.StopDiscovery", &()).await
}

/// Pairs without asking: a device with no keys or screen (headphones, a speaker) pairs by Just Works, and
/// whatever BlueZ would have confirmed is confirmed.
struct Agent;

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {}

    fn request_pin_code(&self, _device: ObjectPath<'_>) -> String {
        "0000".into()
    }

    fn display_pin_code(&self, _device: ObjectPath<'_>, _pincode: String) {}

    fn request_passkey(&self, _device: ObjectPath<'_>) -> u32 {
        0
    }

    fn display_passkey(&self, _device: ObjectPath<'_>, _passkey: u32, _entered: u16) {}

    fn request_confirmation(&self, _device: ObjectPath<'_>, _passkey: u32) {}

    fn request_authorization(&self, _device: ObjectPath<'_>) {}

    fn authorize_service(&self, _device: ObjectPath<'_>, _uuid: String) {}

    fn cancel(&self) {}
}

/// Pairs a device a scan found, through ostrov's own agent, then trusts it (so it reconnects on its own) and
/// connects it.
async fn pair(c: &Ctx, address: &str) -> Res {
    let path = device(c, address).await?;
    let server = c.system.object_server();
    server.at(AGENT_PATH, Agent).await.map_err(err)?;
    let res = paired(c, &path).await;
    let _ = server.remove::<Agent, _>(AGENT_PATH).await;
    res
}

/// Pairs, trusts and connects the device at path, the agent registered with BlueZ for the while.
async fn paired(c: &Ctx, path: &str) -> Res {
    let agent = ObjectPath::from_static_str_unchecked(AGENT_PATH);
    let register = (&agent, "NoInputNoOutput");
    call::<_, ()>(&c.system, BLUEZ, "/org/bluez", "org.bluez.AgentManager1.RegisterAgent", &register).await?;
    let res = async {
        call::<_, ()>(&c.system, BLUEZ, path, "org.bluez.Device1.Pair", &()).await?;
        set_property(&c.system, BLUEZ, path, "org.bluez.Device1.Trusted", true).await?;
        call(&c.system, BLUEZ, path, "org.bluez.Device1.Connect", &()).await
    }
    .await;
    let _: Res = call(&c.system, BLUEZ, "/org/bluez", "org.bluez.AgentManager1.UnregisterAgent", &(&agent,)).await;
    res
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn bt_state() {
        let c = crate::services::Ctx {
            system: zbus::Connection::system().await.unwrap(),
            session: zbus::Connection::session().await.unwrap(),
        };
        println!("{}", super::state(&c).await);
    }
}
