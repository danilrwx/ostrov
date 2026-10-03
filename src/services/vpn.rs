//! VLESS through mihomo's API, OpenVPN through systemd (vpn.go).
use serde_json::Value;
#[allow(unused_imports)]
use super::{Ctx, Kick, Res};

pub async fn vless_state() -> Value { Value::Null }
pub async fn openvpn_state(_c: &Ctx) -> Value { Value::Null }
