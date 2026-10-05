//! ostrov's services (src/services) as ostrov-ctl takes them: the shared helpers themselves, and here the little
//! services/mod.rs gives them without its runtime of all the modules (which brings their GTK widgets along).

#[path = "../../services/dbus.rs"]
pub mod dbus;
#[path = "../../services/rfkill.rs"]
pub mod rfkill;
#[path = "../../services/time.rs"]
pub mod time;

pub use dbus::signals;

pub struct Ctx {
    pub system: zbus::Connection,
    pub session: zbus::Connection,
}

pub type Kick = async_channel::Sender<()>;

pub type Res = Result<(), String>;

/// What a service's command it does not know says: ostrov-ctl's menus give only the words they know.
pub struct Module(pub &'static str);

impl Module {
    pub fn usage(&self) -> String {
        format!("ostrov-ctl: not a {} command", self.0)
    }
}
