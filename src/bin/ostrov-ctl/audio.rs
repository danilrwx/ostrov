//! ostrov's sound (src/modules/audio): PipeWire's devices, the apps recording, the mute, the privacy commands.
#[path = "../../modules/audio/service.rs"]
pub mod service;

pub const MODULE: crate::services::Module = crate::services::Module("audio");
