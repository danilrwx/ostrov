//! The power profile through power-profiles-daemon (power.go).
use serde_json::Value;
#[allow(unused_imports)]
use super::{Ctx, Kick, Res};

pub async fn state(_c: &Ctx) -> Value { Value::Null }
pub async fn cmd(_c: &Ctx, _args: &[&str]) -> Res { Err("todo".into()) }
