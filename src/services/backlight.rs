//! The backlight through sysfs and logind (backlight.go).
use serde_json::Value;
#[allow(unused_imports)]
use super::{Ctx, Kick, Res};

pub fn brightness() -> i64 { 0 }
pub async fn cmd(_c: &Ctx, _args: &[&str]) -> Res { Err("todo".into()) }
