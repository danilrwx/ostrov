//! The player through MPRIS on the session bus (media.go).
use serde_json::Value;
#[allow(unused_imports)]
use super::{Ctx, Kick, Res};

use std::sync::Arc;
pub async fn state(_c: &Ctx) -> Value { Value::Null }
pub async fn cmd(_c: &Ctx, _args: &[&str]) -> Res { Err("todo".into()) }
pub async fn events(_c: Arc<Ctx>, _kick: Kick) {}
