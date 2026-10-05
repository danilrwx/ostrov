//! ostrov's Bluetooth (src/modules/bt): BlueZ's devices and commands.
#[path = "../../modules/bt/service.rs"]
pub mod service;

pub const MODULE: crate::services::Module = crate::services::Module("bt");
