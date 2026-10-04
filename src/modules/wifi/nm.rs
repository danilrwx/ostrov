//! Wi-Fi through NetworkManager, for where it runs instead of iwd: the same state and commands as iwd's.

use std::collections::HashMap;
use std::time::Duration;

use zbus::zvariant::{self, ObjectPath, OwnedObjectPath, OwnedValue};

use super::service::{Network, Wifi, IWD};
use crate::services::dbus::{call, prop, set_property, Objects, Props};
use crate::i18n::{fill, t};
use crate::services::{Ctx, Res};

pub(super) const NM: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const SAVED: &str = "org.freedesktop.NetworkManager.Settings.Connection";

/// A connection's settings, as GetSettings hands them: setting name, then key.
type Settings = HashMap<String, HashMap<String, OwnedValue>>;

/// Whether NetworkManager is the one to speak to: on the system bus while iwd is not. iwd is preferred when both
/// are, NetworkManager then likely running over it.
pub(super) async fn backs(c: &Ctx) -> bool {
    !owned(c, IWD).await && owned(c, NM).await
}

async fn owned(c: &Ctx, name: &str) -> bool {
    call(&c.system, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus.NameHasOwner", &(name,))
        .await
        .unwrap_or(false)
}

/// The array property k (an SSID's bytes, a device's access points), empty when missing.
fn vec<T>(props: Option<&Props>, k: &str) -> Vec<T>
where
    T: TryFrom<zvariant::Value<'static>>,
    T::Error: Into<zvariant::Error>,
{
    let v = props.and_then(|p| p.get(k)).and_then(|v| v.try_clone().ok());
    v.and_then(|v| Vec::try_from(v).ok()).unwrap_or_default()
}

/// The first Wi-Fi device: its path and its Device.Wireless properties.
fn device(objs: &Objects) -> Option<(&OwnedObjectPath, &Props)> {
    objs.iter().filter_map(|(p, i)| Some((p, i.get(WIRELESS)?))).min_by_key(|(p, _)| p.as_str())
}

/// The device's access points with a name (hidden ones have none): path, SSID, properties.
fn aps<'a>(objs: &'a Objects, dev: &Props) -> Vec<(&'a OwnedObjectPath, Vec<u8>, &'a Props)> {
    let paths: Vec<OwnedObjectPath> = vec(Some(dev), "AccessPoints");
    paths
        .iter()
        .filter_map(|p| objs.get_key_value(p))
        .filter_map(|(p, i)| {
            let props = i.get(AP)?;
            let ssid: Vec<u8> = vec(Some(props), "Ssid");
            (!ssid.is_empty()).then_some((p, ssid, props))
        })
        .collect()
}

fn strength(ap: &Props) -> u8 {
    prop(Some(ap), "Strength").unwrap_or(0)
}

/// NetworkManager's strength, 0 to 100, as nmcli's 0 to 4 bars.
fn bars(strength: u8) -> u8 {
    match strength {
        81.. => 4,
        56.. => 3,
        31.. => 2,
        6.. => 1,
        _ => 0,
    }
}

/// An access point's security as iwd names it (open, psk, 8021x, wep), from its key management flags.
fn security(ap: &Props) -> &'static str {
    let flags = |k| prop::<u32>(Some(ap), k).unwrap_or(0);
    let mgmt = flags("WpaFlags") | flags("RsnFlags");
    // KEY_MGMT_802_1X, EAP_SUITE_B_192; PSK, SAE; else the PRIVACY flag of the AP alone is WEP
    if mgmt & (0x200 | 0x2000) != 0 {
        "8021x"
    } else if mgmt & (0x100 | 0x400) != 0 {
        "psk"
    } else if flags("Flags") & 1 != 0 {
        "wep"
    } else {
        "open"
    }
}

/// The state from NetworkManager's properties, its objects and the SSIDs of its saved connections: a network a
/// name, the strongest of its access points, the strongest networks first.
fn read(manager: &Props, objs: &Objects, known: &[Vec<u8>]) -> Wifi {
    let on = prop(Some(manager), "WirelessEnabled").unwrap_or(false)
        && prop(Some(manager), "WirelessHardwareEnabled").unwrap_or(false);
    let mut w = Wifi { on, ssid: String::new(), signal: 0, networks: Vec::new() };
    let Some((_, dev)) = device(objs).filter(|_| on) else { return w };
    let active = prop::<ObjectPath>(Some(dev), "ActiveAccessPoint");
    let mut nets: Vec<(u8, Network)> = Vec::new();
    for (path, ssid, props) in aps(objs, dev) {
        let connected = active.as_ref().is_some_and(|a| a.as_str() == path.as_str());
        let name = String::from_utf8_lossy(&ssid).into_owned();
        if let Some((s, n)) = nets.iter_mut().find(|(_, n)| n.ssid == name) {
            n.connected |= connected;
            *s = (*s).max(strength(props));
            n.signal = bars(*s);
            continue;
        }
        let n = Network {
            ssid: name,
            signal: bars(strength(props)),
            security: security(props).into(),
            known: known.contains(&ssid),
            connected,
        };
        nets.push((strength(props), n));
    }
    nets.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
    w.networks = nets.into_iter().map(|(_, n)| n).collect();
    if let Some(n) = w.networks.iter().find(|n| n.connected) {
        w.ssid.clone_from(&n.ssid);
        w.signal = n.signal;
    }
    w
}

async fn objects(c: &Ctx) -> Result<Objects, String> {
    call(&c.system, NM, "/org/freedesktop", "org.freedesktop.DBus.ObjectManager.GetManagedObjects", &()).await
}

/// The saved connections that are Wi-Fi ones: their paths and SSIDs.
async fn saved(c: &Ctx, objs: &Objects) -> Vec<(OwnedObjectPath, Vec<u8>)> {
    let paths = objs.iter().filter(|(_, i)| i.contains_key(SAVED)).map(|(p, _)| p);
    let all = futures_util::future::join_all(paths.map(|p| async move {
        let s: Settings = call(&c.system, NM, p, &format!("{SAVED}.GetSettings"), &()).await.ok()?;
        let ssid: Vec<u8> = vec(s.get("802-11-wireless"), "ssid");
        (!ssid.is_empty()).then(|| (p.clone(), ssid))
    }));
    all.await.into_iter().flatten().collect()
}

pub(super) async fn state(c: &Ctx) -> Wifi {
    let manager: Result<Props, _> =
        call(&c.system, NM, ROOT, "org.freedesktop.DBus.Properties.GetAll", &("org.freedesktop.NetworkManager",)).await;
    let objs = match objects(c).await {
        Ok(o) => o,
        Err(e) => {
            eprintln!("ostrov: wifi: {e}");
            Objects::new()
        }
    };
    let known: Vec<Vec<u8>> = saved(c, &objs).await.into_iter().map(|(_, s)| s).collect();
    read(&manager.unwrap_or_default(), &objs, &known)
}

pub(super) async fn cmd(c: &Ctx, args: &[&str], passphrase: Option<String>) -> Res {
    let method = |m: &str| format!("org.freedesktop.NetworkManager.{m}");
    match args {
        [w @ ("on" | "off")] => set_property(&c.system, NM, ROOT, &method("WirelessEnabled"), *w == "on").await,
        [w @ ("scan" | "disconnect")] => {
            let objs = objects(c).await?;
            let (dev, _) = device(&objs).ok_or("no Wi-Fi device")?;
            if *w == "scan" {
                let opts: HashMap<&str, zvariant::Value> = HashMap::new();
                call(&c.system, NM, dev, &method("Device.Wireless.RequestScan"), &(opts,)).await
            } else {
                call(&c.system, NM, dev, &method("Device.Disconnect"), &()).await
            }
        }
        ["forget", ssid] => {
            let objs = objects(c).await?;
            let mine: Vec<_> = saved(c, &objs).await.into_iter().filter(|(_, s)| s == ssid.as_bytes()).collect();
            if mine.is_empty() {
                return Err(format!("{ssid:?} is not a known network"));
            }
            for (p, _) in mine {
                call::<_, ()>(&c.system, NM, &p, &format!("{SAVED}.Delete"), &()).await?;
            }
            Ok(())
        }
        ["connect", ssid] => {
            let objs = objects(c).await?;
            let (dev, props) = device(&objs).ok_or(t("Wi-Fi is off"))?;
            let (ap, _, ap_props) = aps(&objs, props)
                .into_iter()
                .filter(|(_, s, _)| s == ssid.as_bytes())
                .max_by_key(|(_, _, p)| strength(p))
                .ok_or_else(|| fill(t("no network {} in sight; scan first"), &[&format!("{ssid:?}")]))?;
            if let Some((conn, _)) = saved(c, &objs).await.into_iter().find(|(_, s)| s == ssid.as_bytes()) {
                let active: OwnedObjectPath =
                    call(&c.system, NM, ROOT, &method("ActivateConnection"), &(&conn, dev, ap)).await?;
                return joined(c, &active).await;
            }
            // NetworkManager completes the rest (the SSID, the key management) from the access point
            let mut settings: HashMap<&str, HashMap<&str, zvariant::Value>> = HashMap::new();
            if security(ap_props) != "open" {
                let psk = passphrase.ok_or("a passphrase is needed on stdin")?;
                settings.insert("802-11-wireless-security", HashMap::from([("psk", zvariant::Value::from(psk))]));
            }
            let (conn, active): (OwnedObjectPath, OwnedObjectPath) =
                call(&c.system, NM, ROOT, &method("AddAndActivateConnection"), &(settings, dev, ap)).await?;
            let res = joined(c, &active).await;
            if res.is_err() {
                // a connection that never joined (a wrong passphrase) is not kept, or it would be known and used
                let _: Res = call(&c.system, NM, &conn, &format!("{SAVED}.Delete"), &()).await;
            }
            res
        }
        _ => Err(super::MODULE.usage()),
    }
}

/// Waits for an activation to end, as iwd's Connect does: Ok once up, an error once NetworkManager gives up.
async fn joined(c: &Ctx, active: &OwnedObjectPath) -> Res {
    // ponytail: State polled, not StateChanged, so why it failed is not told; NetworkManager gives up in under 90 s
    let get = ("org.freedesktop.NetworkManager.Connection.Active", "State");
    for _ in 0..180 {
        let st: Result<OwnedValue, _> = call(&c.system, NM, active, "org.freedesktop.DBus.Properties.Get", &get).await;
        match st.ok().and_then(|v| u32::try_from(v).ok()) {
            Some(2) => return Ok(()),
            Some(0 | 1) => tokio::time::sleep(Duration::from_millis(500)).await,
            _ => return Err(t("could not join (a wrong passphrase?)").into()),
        }
    }
    Err(t("timed out joining").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn props(kv: Vec<(&str, Value<'static>)>) -> Props {
        kv.into_iter().map(|(k, v)| (k.to_owned(), OwnedValue::try_from(v).unwrap())).collect()
    }

    fn path(p: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(p).unwrap()
    }

    fn ap(ssid: &str, strength: u8, rsn: u32) -> HashMap<String, Props> {
        let p = props(vec![
            ("Ssid", Value::from(ssid.as_bytes().to_vec())),
            ("Strength", Value::from(strength)),
            ("Flags", Value::from(u32::from(rsn != 0))),
            ("WpaFlags", Value::from(0u32)),
            ("RsnFlags", Value::from(rsn)),
        ]);
        HashMap::from([(AP.to_owned(), p)])
    }

    fn objs() -> Objects {
        let aps = ["/ap/1", "/ap/2", "/ap/3", "/ap/4"].map(|p| ObjectPath::try_from(p).unwrap());
        let dev = props(vec![
            ("AccessPoints", Value::from(aps.to_vec())),
            ("ActiveAccessPoint", Value::from(ObjectPath::try_from("/ap/2").unwrap())),
        ]);
        Objects::from([
            (path("/dev/wlan0"), HashMap::from([(WIRELESS.to_owned(), dev)])),
            (path("/ap/1"), ap("Home", 90, 0x100)),
            (path("/ap/2"), ap("Home", 50, 0x100)),
            (path("/ap/3"), ap("Cafe", 70, 0)),
            (path("/ap/4"), ap("", 99, 0)),
        ])
    }

    fn manager(on: bool) -> Props {
        props(vec![("WirelessEnabled", Value::from(on)), ("WirelessHardwareEnabled", Value::from(true))])
    }

    #[test]
    fn read_networks() {
        let w = serde_json::to_value(read(&manager(true), &objs(), &[b"Home".to_vec()])).unwrap();
        assert_eq!(
            w,
            serde_json::json!({
                "on": true, "ssid": "Home", "signal": 4,
                "networks": [
                    {"ssid": "Home", "signal": 4, "security": "psk", "known": true, "connected": true},
                    {"ssid": "Cafe", "signal": 3, "security": "open", "known": false, "connected": false},
                ]
            })
        );
    }

    #[test]
    fn read_off() {
        let w = read(&manager(false), &objs(), &[]);
        assert!(!w.on && w.networks.is_empty() && w.ssid.is_empty());
    }

    #[test]
    fn security_flags() {
        let sec = |rsn, flags: u32| {
            security(&props(vec![("RsnFlags", Value::from(rsn)), ("Flags", Value::from(flags))]))
        };
        assert_eq!(sec(0x400u32, 1), "psk");
        assert_eq!(sec(0x200, 1), "8021x");
        assert_eq!(sec(0, 1), "wep");
        assert_eq!(sec(0, 0), "open");
        assert_eq!((bars(100), bars(56), bars(31), bars(6), bars(5)), (4, 3, 2, 1, 0));
    }
}
