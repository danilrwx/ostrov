//! The battery through UPower (battery.go).
use serde_json::Value;
#[allow(unused_imports)]
use super::{Ctx, Kick, Res};

pub async fn state(_c: &Ctx) -> Value { Value::Null }
