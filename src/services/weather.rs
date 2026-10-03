//! The weather from open-meteo (weather.go).
use serde_json::Value;
#[allow(unused_imports)]
use super::{Ctx, Kick, Res};

pub fn state() -> Value { Value::Null }
pub async fn run(_kick: Kick) {}
