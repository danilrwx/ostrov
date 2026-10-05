//! ostrov's power profiles (src/modules/power), power-profiles-daemon's.
#[path = "../../modules/power/service.rs"]
pub mod service;

pub const MODULE: crate::services::Module = crate::services::Module("power");
