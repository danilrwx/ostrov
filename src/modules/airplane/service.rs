//! Airplane Mode: on, every radio soft-blocked at once; off, unblocked again the kinds that were on before it (Wi-Fi
//! and Bluetooth both, say, or Wi-Fi alone), all of them when ostrov has started since.
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::services::rfkill::{self, Radio};
use crate::services::Res;

/// The kinds of radio on when Airplane Mode went on, to unblock when it goes off.
static WERE_ON: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn state() -> Value {
    json!({"on": on(&rfkill::radios())})
}

/// On while there are radios and every one is soft-blocked.
fn on(radios: &[Radio]) -> bool {
    !radios.is_empty() && radios.iter().all(|r| r.soft)
}

/// The kinds with a radio not soft-blocked, each once.
fn unblocked(radios: &[Radio]) -> Vec<String> {
    let mut kinds: Vec<String> = radios.iter().filter(|r| !r.soft).map(|r| r.kind.clone()).collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

/// airplane on|off|toggle.
pub fn cmd(args: &[&str]) -> Res {
    let radios = rfkill::radios();
    let want = match args {
        ["on"] => true,
        ["off"] => false,
        ["toggle"] => !on(&radios),
        _ => return Err(super::MODULE.usage()),
    };
    let mut were = WERE_ON.lock().map_err(|e| e.to_string())?;
    if want {
        if !on(&radios) {
            *were = unblocked(&radios);
        }
        return rfkill::set_blocked(rfkill::ALL, true);
    }
    let kinds = std::mem::take(&mut *were);
    if kinds.is_empty() {
        return rfkill::set_blocked(rfkill::ALL, false);
    }
    kinds.iter().filter_map(|k| rfkill::kind(k)).try_for_each(|k| rfkill::set_blocked(k, false))
}

#[cfg(test)]
mod tests {
    use super::{on, unblocked, Radio};

    fn r(kind: &str, soft: bool) -> Radio {
        Radio { kind: kind.into(), soft, hard: false }
    }

    #[test]
    fn state() {
        assert!(!on(&[]));
        assert!(on(&[r("wlan", true), r("bluetooth", true)]));
        assert!(!on(&[r("wlan", true), r("bluetooth", false)]));
        let radios = [r("wwan", false), r("wlan", false), r("bluetooth", true), r("wlan", false)];
        assert_eq!(unblocked(&radios), ["wlan", "wwan"]);
    }
}
