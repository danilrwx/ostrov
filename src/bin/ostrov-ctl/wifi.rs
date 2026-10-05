//! ostrov's Wi-Fi (src/modules/wifi): iwd's or NetworkManager's networks and commands.
#[path = "../../modules/wifi/nm.rs"]
mod nm;
#[path = "../../modules/wifi/service.rs"]
pub mod service;

pub const MODULE: crate::services::Module = crate::services::Module("wifi");
