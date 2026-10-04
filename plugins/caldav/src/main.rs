//! ostrov's calendars from the network, one of the calendar's sources (`calendar = true`): a CalDAV account's
//! (the login in [plugin.caldav], the app password in the keyring or printed by a command, never in the config)
//! and calendars shared as .ics links. A thread of its own fetches them as the plugin starts, every 15 min (1 min
//! after a failure), on `ostrov plugin caldav refresh` and as its settings change: the events of this month and a
//! week either side, kept here, so ostrov's calendar_events is answered at once; ostrov is told to ask again
//! (`ostrov calendar refresh`) each time they come.

mod dav;
mod ics;

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use ostrov_plugin::{CalendarEvent, Host, Plugin, Value, fill, json, t};

/// What [plugin.caldav] says: one CalDAV account and/or .ics links, the keys [calendar] had.
struct Account {
    caldav_url: String,
    user: String,
    password_command: String,
    ics: Vec<String>,
}

impl Account {
    fn read(host: &Host) -> Account {
        let s = |k| host.setting::<String>(k).unwrap_or_default();
        Account {
            caldav_url: s("caldav_url"),
            user: s("user"),
            password_command: s("password_command"),
            ics: host.setting("ics").unwrap_or_default(),
        }
    }

    fn caldav(&self) -> bool {
        !self.caldav_url.is_empty() && !self.user.is_empty()
    }
}

/// The app password: the one typed (the Test button's), else the one the Settings page keeps in the keyring, else
/// what password_command prints (its last line break off). Blocking: the keyring may ask to be unlocked.
fn password(host: &Host, a: &Account, typed: &str) -> Result<String, String> {
    if !typed.is_empty() {
        return Ok(typed.to_string());
    }
    if let Some(p) = host.secret("password") {
        return Ok(p);
    }
    if a.password_command.is_empty() {
        return Err(t("no password: in Settings, or a password command").into());
    }
    let out =
        std::process::Command::new("sh").args(["-c", &a.password_command]).output().map_err(|e| e.to_string())?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(format!("password_command: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end_matches(['\r', '\n']).to_string())
}

/// The span shown: this month and a week either side, in local seconds.
fn span() -> (i64, i64) {
    let (y, m, _) = ics::civil(ics::now().div_euclid(86400));
    let next = if m == 12 { ics::days(y + 1, 1, 1) } else { ics::days(y, m + 1, 1) };
    ((ics::days(y, m, 1) - 7) * 86400, (next + 7) * 86400)
}

/// Every calendar's events in the span, sorted by their starts, each request given at most `secs`; blocking. A
/// calendar that fails fails it all, so the last events stay rather than some of them.
fn fetch(host: &Host, a: &Account, typed: &str, secs: u64) -> Result<Vec<CalendarEvent>, String> {
    let (from, to) = span();
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(secs)))
        .allow_non_standard_methods(true)
        .build()
        .new_agent();
    let mut docs = Vec::new();
    for url in &a.ics {
        // webcal:// is https:// to a calendar app
        let url = url.replacen("webcal://", "https://", 1);
        let b = agent.get(&url).call().and_then(|mut r| r.body_mut().with_config().limit(dav::LIMIT).read_to_vec());
        docs.push((ics::unfold(&b.map_err(|e| format!("{url}: {e}"))?), String::new()));
    }
    if a.caldav() {
        let pass = password(host, a, typed)?;
        docs.extend(dav::caldav(&agent, &a.caldav_url, &a.user, &pass, from, to)?);
    }
    let mut events = Vec::new();
    for (doc, color) in &docs {
        ics::collect(doc, color, from, to, &mut events);
    }
    events.sort_by(|a, b| a.start.cmp(&b.start));
    Ok(events)
}

/// The events last fetched, and what went wrong the last time ("" if nothing).
#[derive(Default)]
struct Cached {
    events: Vec<CalendarEvent>,
    error: String,
}

fn lock(m: &Mutex<Cached>) -> MutexGuard<'_, Cached> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The fetching thread: the calendars fetched, kept, ostrov told; then a wait of 15 min (1 min after a failure)
/// or until woken. Ends with the plugin.
fn worker(host: Host, cached: Arc<Mutex<Cached>>, wake: Receiver<()>) {
    loop {
        let a = Account::read(&host);
        let got = if a.ics.is_empty() && !a.caldav() { Ok(vec![]) } else { fetch(&host, &a, "", 30) };
        let wait = Duration::from_secs(if got.is_ok() { 15 * 60 } else { 60 });
        match got {
            Ok(events) => *lock(&cached) = Cached { events, error: String::new() },
            Err(e) => {
                host.log(&e);
                lock(&cached).error = e;
            }
        }
        host.kick();
        if let Err(e) = host.run(&["calendar", "refresh"]) {
            host.log(e);
        }
        if let Err(RecvTimeoutError::Disconnected) = wake.recv_timeout(wait) {
            return;
        }
        // a burst of settings' changes is one fetch
        while wake.try_recv().is_ok() {}
    }
}

#[derive(Default)]
struct Caldav {
    cached: Arc<Mutex<Cached>>,
    /// the fetching thread's nudge, once it runs
    wake: Option<Sender<()>>,
}

impl Caldav {
    fn refresh(&mut self, host: &Host) {
        match &self.wake {
            Some(w) => {
                let _ = w.send(());
            }
            None => {
                let (tx, rx) = channel();
                let (h, c) = (host.clone(), self.cached.clone());
                std::thread::spawn(move || worker(h, c, rx));
                self.wake = Some(tx);
            }
        }
    }
}

impl Plugin for Caldav {
    fn on_config(&mut self, host: &Host, _: &Value) {
        host.set_settings_schema(&json!({"sections": [{"title": t("Calendars"), "fields": [
            {"key": "caldav_url", "title": t("CalDAV server"), "type": "url",
             "help": t("https://caldav.icloud.com, Fastmail's, Nextcloud's…")},
            {"key": "user", "title": t("User"), "type": "string"},
            {"key": "password", "title": t("App password"), "type": "secret",
             "help": t("Kept in the keyring, never in the file."),
             "actions": [{"label": t("Test"), "id": "test"}]},
            {"key": "password_command", "title": t("Password command"), "type": "string",
             "help": t("Prints the password when the keyring has none: secret-tool lookup service caldav")},
            {"key": "ics", "title": t("Shared calendars"), "type": "list", "help": t(".ics and webcal:// links.")},
        ]}]}));
        self.refresh(host);
    }

    fn state(&mut self, _: &Host) -> Value {
        let c = lock(&self.cached);
        json!({"events": c.events.len(), "error": c.error})
    }

    fn run(&mut self, host: &Host, args: &[&str], input: Option<&str>) -> Result<String, String> {
        match args {
            ["refresh"] => {
                self.refresh(host);
                Ok(String::new())
            }
            // the password's Test button, what is typed in it as input: fetched now, within ostrov's 10 s
            ["action", "test"] => {
                let n = fetch(host, &Account::read(host), input.unwrap_or("").trim(), 8)?.len();
                self.refresh(host);
                Ok(fill(t("{} events around this month"), &[&n]))
            }
            _ => Err(format!("no command {:?}", args.join(" "))),
        }
    }

    fn calendar_events(&mut self, _: &Host, _from: &str, _to: &str) -> Result<Vec<CalendarEvent>, String> {
        Ok(lock(&self.cached).events.clone())
    }
}

fn main() {
    ostrov_plugin::run(Caldav::default());
}
