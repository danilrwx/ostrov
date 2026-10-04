//! Wi-Fi through iwd, or through NetworkManager (nm.rs) where it runs and iwd does not.

use serde::Serialize;
use serde_json::Value;
use zbus::zvariant::{self, OwnedObjectPath, OwnedValue};

use super::nm;
use crate::services::dbus::{call, err, managed, prop, Objects};
use crate::services::rfkill;
use crate::services::{Ctx, Res};

pub(super) const IWD: &str = "net.connman.iwd";

/// The passphrase agent's object path on the system bus, ostrov's own.
const AGENT_PATH: &str = "/dev/ostrov/wifi_agent";

/// A Wi-Fi network iwd has seen: signal in bars, 0 to 4, as iwctl draws them.
#[derive(Serialize)]
pub(super) struct Network {
    pub(super) ssid: String,
    pub(super) signal: u8,
    pub(super) security: String,
    pub(super) known: bool,
    pub(super) connected: bool,
}

/// The radio's state, the network it is on, and the networks in sight, the strongest first.
#[derive(Serialize)]
pub(super) struct Wifi {
    pub(super) on: bool,
    pub(super) ssid: String,
    pub(super) signal: u8,
    pub(super) networks: Vec<Network>,
}

/// The path of the first device in station mode, None while the radio is off.
fn station(objs: &Objects) -> Option<String> {
    objs.iter().filter(|(_, ifaces)| ifaces.contains_key("net.connman.iwd.Station")).map(|(p, _)| p.to_string()).min()
}

/// iwd's signal, in hundredths of a dBm, as iwctl's 0 to 4 bars.
fn bars(signal: i16) -> u8 {
    match signal {
        -6000.. => 4,
        -6700.. => 3,
        -7500.. => 2,
        -8500.. => 1,
        _ => 0,
    }
}

pub async fn state(c: &Ctx) -> Value {
    if nm::backs(c).await {
        return serde_json::to_value(nm::state(c).await).unwrap_or_default();
    }
    let mut w = Wifi { on: !rfkill::blocked("wlan"), ssid: String::new(), signal: 0, networks: Vec::new() };
    if let Err(e) = networks(c, &mut w).await {
        eprintln!("ostrov: wifi: {e}");
    }
    serde_json::to_value(w).unwrap_or_default()
}

/// The station's networks into w; on an error what was read before it stays.
async fn networks(c: &Ctx, w: &mut Wifi) -> Res {
    let objs = managed(&c.system, IWD).await?;
    let Some(st) = station(&objs) else { return Ok(()) };
    let ordered: Vec<(OwnedObjectPath, i16)> =
        call(&c.system, IWD, &st, "net.connman.iwd.Station.GetOrderedNetworks", &()).await?;
    for (path, signal) in ordered {
        let props = objs.get(&path).and_then(|i| i.get("net.connman.iwd.Network"));
        let n = Network {
            ssid: prop(props, "Name").unwrap_or_default(),
            signal: bars(signal),
            security: prop(props, "Type").unwrap_or_default(),
            known: props.is_some_and(|p| p.contains_key("KnownNetwork")),
            connected: prop(props, "Connected").unwrap_or_default(),
        };
        if n.connected {
            w.ssid.clone_from(&n.ssid);
            w.signal = n.signal;
        }
        w.networks.push(n);
    }
    Ok(())
}

/// The path of the network named ssid among those in sight.
async fn network(c: &Ctx, ssid: &str) -> Result<String, String> {
    let objs = managed(&c.system, IWD).await?;
    objs.iter()
        .find(|(_, ifaces)| prop::<&str>(ifaces.get("net.connman.iwd.Network"), "Name") == Some(ssid))
        .map(|(p, _)| p.to_string())
        .ok_or_else(|| format!("no network {ssid:?} in sight; scan first"))
}

/// Answers iwd's passphrase request with the one the command was given, for a network iwd does not know yet.
struct Agent {
    passphrase: Option<String>,
}

#[zbus::interface(name = "net.connman.iwd.Agent")]
impl Agent {
    fn request_passphrase(&self, _network: zvariant::ObjectPath<'_>) -> zbus::fdo::Result<String> {
        self.passphrase.clone().ok_or_else(|| zbus::fdo::Error::Failed("a passphrase is needed on stdin".into()))
    }

    fn release(&self) {}

    fn cancel(&self, _reason: String) {}
}

/// A command's stdin as a passphrase: read to its end, a line, "" when only its newline; nothing at all is none.
pub(super) fn passphrase(input: Option<String>) -> Option<String> {
    input.filter(|s| !s.is_empty()).map(|s| s.trim_end_matches(['\r', '\n']).to_owned())
}

pub async fn cmd(c: &Ctx, args: &[&str], input: Option<String>) -> Res {
    if nm::backs(c).await {
        return nm::cmd(c, args, passphrase(input)).await;
    }
    match args {
        [w @ ("on" | "off")] => rfkill::set_blocked(rfkill::WLAN, *w == "off"),
        [w @ ("scan" | "disconnect")] => {
            let objs = managed(&c.system, IWD).await?;
            let st = station(&objs).ok_or("Wi-Fi is off")?;
            let method = if *w == "scan" { "Scan" } else { "Disconnect" };
            call(&c.system, IWD, &st, &format!("net.connman.iwd.Station.{method}"), &()).await
        }
        ["forget", ssid] => {
            let path = network(c, ssid).await?;
            let get = ("net.connman.iwd.Network", "KnownNetwork");
            let known: OwnedValue = call(&c.system, IWD, &path, "org.freedesktop.DBus.Properties.Get", &get)
                .await
                .map_err(|_| format!("{ssid:?} is not a known network"))?;
            let kpath = OwnedObjectPath::try_from(known).map_err(|e| e.to_string())?;
            call(&c.system, IWD, &kpath, "net.connman.iwd.KnownNetwork.Forget", &()).await
        }
        ["connect", ssid] => {
            let path = network(c, ssid).await?;
            let passphrase = passphrase(input);
            let server = c.system.object_server();
            // the agent of a connect still waiting is replaced, as another ostrov's would stand in for it
            let _ = server.remove::<Agent, _>(AGENT_PATH).await;
            server.at(AGENT_PATH, Agent { passphrase }).await.map_err(err)?;
            let res = connect(c, &path).await;
            let _ = server.remove::<Agent, _>(AGENT_PATH).await;
            res
        }
        _ => Err(super::MODULE.usage()),
    }
}

/// Connects to the network at path, ostrov's agent registered with iwd for the while.
async fn connect(c: &Ctx, path: &str) -> Res {
    let agent = zvariant::ObjectPath::from_static_str_unchecked(AGENT_PATH);
    call::<_, ()>(&c.system, IWD, "/net/connman/iwd", "net.connman.iwd.AgentManager.RegisterAgent", &(&agent,)).await?;
    let res = call(&c.system, IWD, path, "net.connman.iwd.Network.Connect", &()).await;
    let _: Res =
        call(&c.system, IWD, "/net/connman/iwd", "net.connman.iwd.AgentManager.UnregisterAgent", &(&agent,)).await;
    res
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn wifi_state() {
        let c = crate::services::Ctx {
            system: zbus::Connection::system().await.unwrap(),
            session: zbus::Connection::session().await.unwrap(),
        };
        println!("{}", super::state(&c).await);
    }
}
