//! What a part of the desktop does off GTK's thread (a plugin's process): its state as JSON, its commands, its
//! worker running for as long as it does and kicking whenever its state is to be read again. GTK's side holds it
//! as Arc<dyn Backend>.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

pub use crate::services::Kick;

pub type BoxFut<T> = Pin<Box<dyn Future<Output = T> + Send>>;

pub trait Backend: Send + Sync {
    /// The state now.
    fn state(&self) -> BoxFut<Value>;
    /// A command's words and what it would read on its stdin (a passphrase): its output, or what went wrong. Its
    /// output rather than services::Res's nothing, since a plugin's command may answer.
    fn run(&self, args: &[String], input: Option<String>) -> BoxFut<Result<String, String>>;
    /// What runs for as long as the part does, kick sent whenever its state changed.
    fn worker(&self, kick: Kick) -> BoxFut<()>;
}
