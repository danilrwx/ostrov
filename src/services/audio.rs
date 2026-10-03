//! The sound devices through pw-dump, the headset's profile through wpctl (audio.go).
use serde_json::Value;
#[allow(unused_imports)]
use super::{Ctx, Kick, Res};

pub async fn state() -> Value { Value::Null }
pub async fn headset() -> Res { Err("todo".into()) }
pub async fn events(_kick: Kick) {}
