//! The night light through Hyprland's screen shader (night.go).
use serde_json::Value;
#[allow(unused_imports)]
use super::{Ctx, Kick, Res};

pub fn state() -> Value { Value::Null }
pub async fn cmd(_args: &[&str]) -> Res { Err("todo".into()) }
pub async fn apply_now() -> Res { Ok(()) }
