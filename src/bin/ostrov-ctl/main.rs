//! ostrov-ctl: for a desktop on dwl rather than Hyprland, ostrov's services without its GTK: the status line
//! for dwl's bar (status.rs) and wmenu's menus of what is rarely needed (menu.rs). The services' code is
//! ostrov's own, its files taken in by path (services.rs and a file a module); nothing of GTK is linked.

#[path = "../../config.rs"]
#[allow(dead_code)]
mod config;
#[path = "../../i18n.rs"]
#[allow(dead_code)]
mod i18n;

#[allow(dead_code)]
mod audio;
#[allow(dead_code)]
mod battery;
#[allow(dead_code)]
mod bt;
mod menu;
#[allow(dead_code)]
mod power;
#[allow(dead_code)]
mod services;
mod status;
#[allow(dead_code)]
mod wifi;

/// The home, config.rs's.
mod hub {
    pub fn home() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
    }
}

use std::sync::Arc;

use services::Ctx;

const HELP: &str = "usage: ostrov-ctl status [--watch] | menu MENU | click BLOCK
  status            the status line for dwl's bar, once
  status --watch    a line on every change: ostrov-ctl status --watch | dwl
  menu MENU         a menu in wmenu, its pick done: wifi|bt|audio|power|main, or a block's
  click BLOCK       what a click on a block of the status does: battery (the next power profile), notes (Do Not
                    Disturb), privacy (the mic muted), mute (the sound back), net and clock (a menu)";

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let menus = format!("{}|net|mute|privacy|battery|clock", menu::MENUS);
    let known = match args[..] {
        ["status"] | ["status", "--watch"] | ["click", _] => true,
        ["menu", m] => menus.split('|').any(|k| k == m),
        _ => false,
    };
    if !known {
        println!("{HELP}");
        return if args.is_empty() || args == ["help"] { 0 } else { 2 }.into();
    }
    let Ok(rt) = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() else {
        return 1.into();
    };
    let res = rt.block_on(async {
        let (system, session) = tokio::join!(zbus::Connection::system(), zbus::Connection::session());
        let (system, session) = (system.map_err(|e| e.to_string())?, session.map_err(|e| e.to_string())?);
        let c = Arc::new(Ctx { system, session });
        let res = match args[..] {
            ["status"] => Ok(println!("{}", status::line(&status::read(&c).await))),
            ["status", "--watch"] => Ok(status::watch(c.clone()).await),
            ["menu", m] => menu::menu(&c, m).await,
            ["click", b] => menu::click(&c, b).await,
            _ => Ok(()),
        };
        if let Err(e) = &res {
            tell(&c, e).await;
        }
        res
    });
    // the runtime let go of without waiting on pw-dump's reader, which reads on until its pipe closes
    rt.shutdown_background();
    match res {
        Ok(()) => 0.into(),
        Err(e) => {
            eprintln!("ostrov-ctl: {e}");
            1.into()
        }
    }
}

/// A command's failure as a notification too: ostrov-ctl runs from dwl's keys and clicks, no terminal to see it.
async fn tell(c: &Ctx, e: &str) {
    let hints: std::collections::HashMap<&str, zbus::zvariant::Value> = Default::default();
    let body = ("ostrov-ctl", 0u32, "", "ostrov-ctl", e, Vec::<&str>::new(), hints, -1i32);
    let path = "/org/freedesktop/Notifications";
    let notify = c.session.call_method(Some(status::MAKO), path, Some(status::MAKO), "Notify", &body);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), notify).await;
}
