//! Wi-Fi through iwd (wifi.go).
use serde_json::Value;
#[allow(unused_imports)]
use super::{Ctx, Kick, Res};

pub async fn state(_c: &Ctx) -> Value { Value::Null }
pub async fn cmd(_c: &Ctx, _args: &[&str], _input: Option<String>) -> Res { Err("todo".into()) }
