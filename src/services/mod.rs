//! The desktop's state and switches, wmd (~/dotfiles/wmd, its Go original) ported into ostrov: from the services
//! themselves rather than their command-line tools. Wi-Fi through iwd and Bluetooth through BlueZ (D-Bus), the
//! radios through /dev/rfkill, the power profile through power-profiles-daemon, the battery through UPower, the
//! backlight through logind, the keyboard layout through the compositor, the sound devices through pw-dump, the
//! player through MPRIS, VLESS through mihomo's API, OpenVPN through systemd, bin/theme's pick from its state
//! files, the night light through Hyprland's screen shader, the weather from open-meteo.
//!
//! The state is wmd watch's JSON, field for field, so what draws it reads the same; the commands are wmd's
//! (wifi connect SSID, bt pair ADDR, night mode sun, ...). It all runs on a Tokio runtime of its own (start).

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

pub mod audio;
pub mod backlight;
mod battery;
mod bt;
mod calendar;
mod dbus;
mod games;
mod displays;
mod keymap;
mod location;
mod media;
mod night;
mod power;
mod rfkill;
mod theme;
mod vpn;
mod weather;
mod wifi;

/// What every service reaches the desktop through: the system bus and the session bus.
pub struct Ctx {
    pub system: zbus::Connection,
    pub session: zbus::Connection,
}

/// A kick down to the watcher: something changed, the state is to be read again.
pub type Kick = async_channel::Sender<()>;

/// A command's outcome: Ok, or what went wrong, said to the user as it is.
pub type Res = Result<(), String>;

pub const USAGE: &str = "wifi on|off|scan|disconnect|connect SSID|forget SSID | bt on|off|scan|connect ADDR|disconnect ADDR|\
pair ADDR|forget ADDR | headset | audio volume ID LEVEL | power set PROFILE | brightness PERCENT | night mode off|on|time|sun|time FROM TO|temp K|\
preview K|apply | location CITY|LAT LON | media play-pause|next|previous | displays set NAME MODE POSITION SCALE|\
on NAME|off NAME|mirror NAME OF|save NAME|load NAME|delete NAME | calendar refresh";

/// The state now, as wmd watch prints it.
pub async fn state(c: &Ctx) -> Value {
    let (wifi, bt, power, audio, battery, openvpn, media, keymap, vless) = tokio::join!(
        wifi::state(c),
        bt::state(c),
        power::state(c),
        audio::state(),
        battery::state(c),
        vpn::openvpn_state(c),
        media::state(c),
        keymap::keymap(),
        vpn::vless_state(),
    );
    json!({
        "wifi": wifi,
        "bt": bt,
        "power": power,
        "brightness": backlight::brightness(),
        "keymap": keymap,
        "audio": audio,
        "vless": vless,
        "openvpn": openvpn,
        "theme": theme::state(),
        "night": night::state(),
        "weather": weather::state(),
        "battery": battery,
        "media": media,
        "displays": displays::state(),
        "calendar": calendar::state(),
    })
}

/// A command, wmd's words; input is what it would read from its stdin (a Wi-Fi passphrase).
pub async fn run(c: &Ctx, args: &[String], input: Option<String>) -> Res {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let Some((first, rest)) = args.split_first() else { return Err(USAGE.into()) };
    match *first {
        "wifi" => wifi::cmd(c, rest, input).await,
        "bt" => bt::cmd(c, rest).await,
        "headset" => audio::headset().await,
        "audio" => audio::cmd(rest).await,
        "power" => power::cmd(c, rest).await,
        "brightness" => backlight::cmd(c, rest).await,
        "night" => night::cmd(rest).await,
        "location" => location::cmd(rest).await,
        "media" => media::cmd(c, rest).await,
        "displays" => displays::cmd(rest).await,
        "calendar" => calendar::cmd(rest).await,
        _ => Err(USAGE.into()),
    }
}

/// The state, then again 100 ms after the last of a burst of changes: iwd, BlueZ, power-profiles-daemon and
/// UPower signal theirs on D-Bus, the compositor its layout switches, PipeWire and the players theirs, the
/// weather every quarter of an hour. Every 3 s besides, for what tells nothing (a network's strength, the
/// backlight, VLESS, OpenVPN, the theme), the night light kept to its schedule then too. Only a state that
/// differs from the last one sent goes out.
pub async fn watch(c: Arc<Ctx>, out: async_channel::Sender<Value>) {
    let (kick, kicks) = async_channel::unbounded::<()>();
    tokio::spawn(signals(c.clone(), kick.clone()));
    tokio::spawn(keymap::events(kick.clone()));
    tokio::spawn(weather::run(kick.clone()));
    tokio::spawn(calendar::run(kick.clone()));
    tokio::spawn(media::events(c.clone(), kick.clone()));
    tokio::spawn(audio::events(kick.clone()));
    tokio::spawn(displays::events(kick.clone()));
    tokio::spawn(games::run(c.clone()));

    let mut last = Value::Null;
    let mut emit = async |c: &Ctx| {
        let s = state(c).await;
        if s != last {
            last = s.clone();
            let _ = out.send(s).await;
        }
    };
    emit(&c).await;
    let mut tick = tokio::time::interval(Duration::from_secs(3));
    tick.tick().await;
    let settle = tokio::time::sleep(Duration::from_secs(3600));
    tokio::pin!(settle);
    let mut settling = false;
    loop {
        tokio::select! {
            Ok(()) = kicks.recv() => {
                settle.as_mut().reset(tokio::time::Instant::now() + Duration::from_millis(100));
                settling = true;
            }
            () = &mut settle, if settling => {
                settling = false;
                emit(&c).await;
            }
            _ = tick.tick() => {
                audio::scan_cameras().await;
                if let Err(e) = night::apply_now().await {
                    eprintln!("ostrov: night: {e}");
                }
                emit(&c).await;
            }
        }
    }
}

/// Every signal of iwd's, BlueZ's, power-profiles-daemon's and UPower's on the system bus, as a kick.
async fn signals(c: Arc<Ctx>, kick: Kick) {
    use futures_util::StreamExt;
    let mut streams = Vec::new();
    for sender in ["net.connman.iwd", "org.bluez", "net.hadess.PowerProfiles", "org.freedesktop.UPower"] {
        let Ok(rule) = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).sender(sender).map(|b| b.build()) else {
            continue;
        };
        if let Ok(s) = zbus::MessageStream::for_match_rule(rule, &c.system, None).await {
            streams.push(s);
        }
    }
    let mut all = futures_util::stream::select_all(streams);
    while all.next().await.is_some() {
        let _ = kick.send(()).await;
    }
}

/// The services on a thread and Tokio runtime of their own: their state down state_tx as it changes, the
/// commands from cmds run as they come, each one's outcome back down its own channel.
pub fn start(state_tx: async_channel::Sender<Value>, cmds: async_channel::Receiver<(Vec<String>, Option<String>, async_channel::Sender<Res>)>) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_multi_thread().enable_all().build() else { return };
        rt.block_on(async move {
            let (Ok(system), Ok(session)) = (zbus::Connection::system().await, zbus::Connection::session().await) else {
                eprintln!("ostrov: services: no D-Bus");
                return;
            };
            let c = Arc::new(Ctx { system, session });
            tokio::spawn(watch(c.clone(), state_tx));
            while let Ok((args, input, done)) = cmds.recv().await {
                let c = c.clone();
                tokio::spawn(async move {
                    let _ = done.send(run(&c, &args, input).await).await;
                });
            }
        });
    });
}
