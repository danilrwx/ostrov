//! bin/theme's pick from its state files (theme.go).
use serde_json::Value;
#[allow(unused_imports)]
use super::{Ctx, Kick, Res};

pub fn state() -> Value { Value::Null }
