//! VLESS through mihomo's API, OpenVPN through systemd (vpn.go).

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use super::Ctx;

/// mihomo's state as bin/vless runs it: off, or tun (TUN) or proxy (the system proxy); the mode, rule or global;
/// the profile picked in the PROXY group and the ones to pick from. From mihomo's API, which answers only while
/// it runs; bin/vless itself still does the switching.
#[derive(Serialize)]
struct Vless {
    how: String,
    mode: String,
    profile: String,
    profiles: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Config {
    mode: String,
    tun: Tun,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Tun {
    enable: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Group {
    now: String,
    all: Option<Vec<String>>,
}

/// A JSON answer from mihomo's local API, which answers at once or is not running: 500 ms at most, no proxy,
/// and an error status's body read all the same, as Go's client does.
async fn get_json<T: DeserializeOwned + Send + 'static>(url: &'static str) -> Option<T> {
    let get = move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(500)))
            .http_status_as_error(false)
            .proxy(None)
            .build()
            .into();
        let body = agent.get(url).call().ok()?.body_mut().read_to_string().ok()?;
        serde_json::from_str(&body).ok()
    };
    tokio::task::spawn_blocking(get).await.ok().flatten()
}

pub async fn vless_state() -> Value {
    let mut v = Vless { how: "off".into(), mode: String::new(), profile: String::new(), profiles: Vec::new() };
    if let Some(cfg) = get_json::<Config>("http://127.0.0.1:9090/configs").await {
        v.how = if cfg.tun.enable { "tun" } else { "proxy" }.into();
        v.mode = cfg.mode;
        if let Some(group) = get_json::<Group>("http://127.0.0.1:9090/proxies/PROXY").await {
            v.profile = group.now;
            v.profiles.extend(group.all.into_iter().flatten());
        }
    }
    serde_json::to_value(v).unwrap_or_default()
}

/// The openvpn-client@ unit up or coming up (bin/openvpn-ctl), "" while none is, whether it is through, and the
/// profiles in private/config/openvpn.
#[derive(Serialize, Default)]
struct OpenVpn {
    profile: String,
    ready: bool,
    profiles: Vec<String>,
}

type Unit = (String, String, String, String, String, String, OwnedObjectPath, u32, String, OwnedObjectPath);

pub async fn openvpn_state(c: &Ctx) -> Value {
    let mut o = OpenVpn::default();
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let dir = home.join("dotfiles/private/config/openvpn");
    for f in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if let Some(name) = f.file_name().to_string_lossy().strip_suffix(".ovpn") {
            o.profiles.push(name.into());
        }
    }
    o.profiles.sort();

    let systemd = Some("org.freedesktop.systemd1");
    let units = c
        .system
        .call_method(
            systemd,
            "/org/freedesktop/systemd1",
            Some("org.freedesktop.systemd1.Manager"),
            "ListUnitsByPatterns",
            &(["active", "activating"].as_slice(), ["openvpn-client@*.service"].as_slice()),
        )
        .await;
    let units: Vec<Unit> = units.ok().and_then(|r| r.body().deserialize().ok()).unwrap_or_default();
    if let Some(u) = units.first() {
        let name = u.0.strip_prefix("openvpn-client@").unwrap_or(&u.0);
        o.profile = name.strip_suffix(".service").unwrap_or(name).into();
        // through once openvpn tells systemd so (sd_notify's status)
        let status = c
            .system
            .call_method(
                systemd,
                &u.6,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.freedesktop.systemd1.Service", "StatusText"),
            )
            .await;
        let text = status.ok().and_then(|r| r.body().deserialize::<OwnedValue>().ok());
        let text = text.and_then(|v| String::try_from(v).ok());
        o.ready = text.is_some_and(|t| t.contains("Initialization Sequence Completed"));
    }
    serde_json::to_value(o).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn vpn_state() {
        let (system, session) = (zbus::Connection::system().await.unwrap(), zbus::Connection::session().await.unwrap());
        let c = super::Ctx { system, session };
        println!("vless {}", super::vless_state().await);
        println!("openvpn {}", super::openvpn_state(&c).await);
    }
}
