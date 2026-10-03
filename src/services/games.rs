//! Games played in the performance profile, gamemode's work without it: while a game's window has the focus
//! (its class one of config.toml's [games] classes, "steam_app_*" any of Steam's), the power profile is the one
//! games want, the one before put back as the focus leaves it. Only a profile set here is put back: one picked
//! by hand before the game stays. Hyprland's event socket read on a thread of its own, as keymap's.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use super::{power, Ctx};
use crate::wm::hypr_socket;

/// A window's class one of the games: equal, or a prefix before a trailing *.
fn is_game(class: &str, classes: &[String]) -> bool {
    classes.iter().any(|c| match c.strip_suffix('*') {
        Some(prefix) => class.starts_with(prefix),
        None => class == c,
    })
}

pub async fn run(c: Arc<Ctx>) {
    let Some(sock) = hypr_socket(".socket2.sock") else { return };
    let (tx, rx) = async_channel::unbounded::<String>();
    // the class of every window focused
    tokio::task::spawn_blocking(move || {
        let Ok(s) = UnixStream::connect(sock) else { return };
        for line in BufReader::new(s).lines().map_while(Result::ok) {
            let Some(rest) = line.strip_prefix("activewindow>>") else { continue };
            let class = rest.split_once(',').map_or(rest, |(c, _)| c);
            if tx.send_blocking(class.to_string()).is_err() {
                return;
            }
        }
    });
    // the profile before the game, while a game has the focus
    let mut before: Option<String> = None;
    while let Ok(class) = rx.recv().await {
        let games = crate::config::load().games;
        let game = is_game(&class, &games.classes);
        if game && before.is_none() {
            let now = power::state(&c).await["active"].as_str().unwrap_or_default().to_string();
            if now != games.profile && !now.is_empty() {
                if let Err(e) = power::cmd(&c, &["set", &games.profile]).await {
                    eprintln!("ostrov: games: {e}");
                    continue;
                }
                before = Some(now);
            }
        } else if !game {
            if let Some(p) = before.take() {
                if let Err(e) = power::cmd(&c, &["set", &p]).await {
                    eprintln!("ostrov: games: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn is_game() {
        let classes = ["dota2".to_string(), "steam_app_*".to_string()];
        assert!(super::is_game("dota2", &classes));
        assert!(super::is_game("steam_app_570", &classes));
        assert!(!super::is_game("dota", &classes));
        assert!(!super::is_game("Alacritty", &classes));
    }
}
