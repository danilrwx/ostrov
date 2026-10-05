//! The system's updates: the packages waiting, as the package manager here lists them without root (apt's list
//! of upgradable packages, kept fresh by Ubuntu's and Debian's apt-daily; pacman-contrib's checkupdates; dnf's
//! check-update), looked at every hour on a thread; the update itself run in $TERMINAL, where sudo asks.

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ostrov_plugin::{Host, Plugin, Value, fill, json, t};

const ICON: &str = "software-update-available-symbolic";
const USAGE: &str = "usage: ostrov plugin updates list | check | update";
/// The timer taking in a check's result.
const DONE: u32 = 1;
/// The timer starting the next check.
const DUE: u32 = 2;
/// How many packages the menu names before "and N more".
const SHOWN: usize = 12;

/// A package manager: the check printing what waits, how its lines are read, the update.
struct Manager {
    check: &'static [&'static str],
    /// the exit status that still means success (dnf's 100: updates waiting)
    also_ok: i32,
    parse: fn(&str) -> Vec<String>,
    update: &'static str,
}

const MANAGERS: [(&str, Manager); 3] = [
    ("apt", Manager { check: &["apt", "list", "--upgradable"], also_ok: 0, parse: apt, update: "sudo apt upgrade" }),
    ("checkupdates", Manager { check: &["checkupdates"], also_ok: 2, parse: first_word, update: "sudo pacman -Syu" }),
    ("dnf", Manager { check: &["dnf", "-q", "check-update"], also_ok: 100, parse: dnf, update: "sudo dnf upgrade" }),
];

/// apt's "name/suite version arch [upgradable from: old]" lines, "Listing..." and warnings left out.
fn apt(out: &str) -> Vec<String> {
    out.lines().filter_map(|l| l.split_once('/').map(|(n, _)| n)).filter(|n| !n.contains(' ')).map(String::from).collect()
}

/// A package's name, the first word of each line (checkupdates' "name old -> new", a command of the user's).
fn first_word(out: &str) -> Vec<String> {
    out.lines().filter_map(|l| l.split_whitespace().next()).map(String::from).collect()
}

/// dnf's "name.arch version repo" lines, its "Obsoleting Packages" section and blank lines left out.
fn dnf(out: &str) -> Vec<String> {
    out.lines()
        .take_while(|l| !l.starts_with("Obsoleting"))
        .filter(|l| l.split_whitespace().count() == 3)
        .filter_map(|l| l.split_whitespace().next()?.rsplit_once('.').map(|(n, _)| n.to_string()))
        .collect()
}

fn found(prog: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(prog).is_file()))
}

/// The packages waiting: the user's check command (through sh, a package a line, its name first), else the
/// manager found here.
fn check(own: &str) -> Result<Vec<String>, String> {
    if !own.trim().is_empty() {
        let out = Command::new("sh").args(["-c", own]).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
        return Ok(first_word(&String::from_utf8_lossy(&out.stdout)));
    }
    let (_, m) = MANAGERS.iter().find(|(prog, _)| found(prog)).ok_or_else(|| {
        t("no package manager known here (apt, pacman with pacman-contrib, dnf); set check and update").to_string()
    })?;
    let out = Command::new(m.check[0])
        .args(&m.check[1..])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", m.check[0]))?;
    match out.status.code() {
        // checkupdates' 2 is nothing waiting, dnf's 100 something: either way its output says what
        Some(c) if c == 0 || c == m.also_ok => Ok((m.parse)(&String::from_utf8_lossy(&out.stdout))),
        _ => Err(format!("{}: {}", m.check.join(" "), out.status)),
    }
}

/// The update, the user's command else the manager's, in $TERMINAL (x-terminal-emulator without it), left open
/// until Enter so what it said can be read.
fn update(own: &str) -> Result<(), String> {
    let cmd = if own.trim().is_empty() {
        MANAGERS.iter().find(|(prog, _)| found(prog)).map(|(_, m)| m.update.to_string()).ok_or("no package manager")?
    } else {
        own.to_string()
    };
    let term = std::env::var("TERMINAL").ok().filter(|t| !t.is_empty()).unwrap_or("x-terminal-emulator".into());
    let script = format!("{cmd}; echo; printf '%s' 'Enter closes this window'; read _");
    Command::new(term).args(["-e", "sh", "-c", &script]).spawn().map(drop).map_err(|e| e.to_string())
}

struct Updates {
    waiting: Option<Result<Vec<String>, String>>,
    /// a check's result from its thread, taken in at DONE
    result: Arc<Mutex<Option<Result<Vec<String>, String>>>>,
    checking: bool,
    started: bool,
}

impl Updates {
    fn check(&mut self, host: &Host) {
        if self.checking {
            return;
        }
        self.checking = true;
        host.kick();
        let (h, slot, own) = (host.clone(), self.result.clone(), host.setting::<String>("check").unwrap_or_default());
        std::thread::spawn(move || {
            let r = check(&own);
            if let Ok(mut s) = slot.lock() {
                *s = Some(r);
            }
            h.set_timer(0, DONE);
        });
    }

    fn count(&self) -> usize {
        self.waiting.as_ref().and_then(|w| w.as_ref().ok()).map_or(0, Vec::len)
    }
}

impl Plugin for Updates {
    fn on_config(&mut self, host: &Host, _: &Value) {
        host.set_settings_schema(&json!({"sections": [{"title": t("Updates"), "fields": [
            {"key": "every", "title": t("Every (minutes)"), "type": "number", "default": 60, "min": 10, "max": 1440},
            {"key": "check", "title": t("Check command"), "type": "string", "default": "",
             "help": t("prints the packages waiting, one a line, the name first; empty: apt's, pacman's or dnf's, whichever is here")},
            {"key": "update", "title": t("Update command"), "type": "string", "default": "",
             "help": t("run in $TERMINAL; empty: sudo apt upgrade, sudo pacman -Syu or sudo dnf upgrade")},
        ]}]}));
        if !self.started {
            self.started = true;
            self.check(host);
        }
    }

    fn on_timer(&mut self, host: &Host, id: u32) {
        match id {
            DONE => {
                self.checking = false;
                if let Some(r) = self.result.lock().ok().and_then(|mut s| s.take()) {
                    if let Err(e) = &r {
                        host.log(e);
                    }
                    self.waiting = Some(r);
                }
                let every = host.setting::<f64>("every").unwrap_or(60.0).clamp(10.0, 1440.0) as u64;
                host.set_timer(Duration::from_secs(every * 60).as_millis() as u32, DUE);
                host.kick();
            }
            DUE => self.check(host),
            _ => {}
        }
    }

    fn state(&mut self, _: &Host) -> Value {
        match &self.waiting {
            Some(Ok(w)) => json!({"count": w.len(), "packages": w}),
            Some(Err(e)) => json!({"error": e}),
            None => json!({}),
        }
    }

    fn run(&mut self, host: &Host, args: &[&str], _: Option<&str>) -> Result<String, String> {
        match args {
            ["list"] => match &self.waiting {
                Some(Ok(w)) => Ok(w.join("\n")),
                Some(Err(e)) => Err(e.clone()),
                None => Err(t("Checking…").into()),
            },
            ["check"] => {
                self.check(host);
                Ok(String::new())
            }
            ["update"] => update(&host.setting::<String>("update").unwrap_or_default()).map(|_| String::new()),
            _ => Err(USAGE.into()),
        }
    }

    fn render(&mut self, _: &Host, widget: &str) -> Option<Value> {
        let n = self.count();
        if widget == "updates#bar" {
            return Some(json!({"type": "image", "icon": ICON, "active": n > 0}));
        }
        let sub = match (&self.waiting, self.checking) {
            (_, true) | (None, _) => t("Checking…").to_string(),
            (Some(Err(e)), _) => e.clone(),
            (Some(Ok(w)), _) if w.is_empty() => t("Up to date").to_string(),
            (Some(Ok(w)), _) => fill(t("{} waiting"), &[&w.len()]),
        };
        let mut menu = vec![];
        if let Some(Ok(w)) = &self.waiting {
            for p in w.iter().take(SHOWN) {
                menu.push(json!({"type": "label", "text": p}));
            }
            if w.len() > SHOWN {
                menu.push(json!({"type": "label", "class": "dim", "text": fill(t("and {} more"), &[&(w.len() - SHOWN)])}));
            }
        }
        menu.push(json!({"type": "box", "orientation": "horizontal", "children": [
            {"type": "button", "id": "update", "icon": ICON, "label": t("Update")},
            {"type": "button", "id": "check", "icon": "view-refresh-symbolic", "label": t("Check Now")},
        ]}));
        Some(json!({"type": "toggle", "id": "updates", "icon": ICON, "title": t("Updates"), "sub": sub, "on": n > 0,
            "menu": {"type": "box", "children": menu}}))
    }

    fn on_event(&mut self, host: &Host, _: &str, node: &str, _: &str, _: &str) {
        match node {
            "update" => {
                if let Err(e) = update(&host.setting::<String>("update").unwrap_or_default()) {
                    host.log(e);
                }
            }
            // the tile's switch and Check Now look again
            _ => self.check(host),
        }
    }
}

fn main() {
    ostrov_plugin::run(Updates { waiting: None, result: Arc::default(), checking: false, started: false });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists() {
        let a = "Listing...\nfirefox/resolute-updates 140.0+build1 amd64 [upgradable from: 139.0]\nlibc6/resolute-security 2.42-1 amd64 [upgradable from: 2.41-9]\n";
        assert_eq!(apt(a), ["firefox", "libc6"]);
        assert_eq!(apt("Listing... Done\n"), Vec::<String>::new());
        assert_eq!(first_word("linux 6.9.1-1 -> 6.9.2-1\nmesa 24.1 -> 24.2\n"), ["linux", "mesa"]);
        let d = "\nkernel.x86_64   6.10.3-200.fc40   updates\nmesa-libGL.x86_64  24.1.5-1.fc40  updates\nObsoleting Packages\nold.noarch 1-1 updates\n";
        assert_eq!(dnf(d), ["kernel", "mesa-libGL"]);
    }
}
