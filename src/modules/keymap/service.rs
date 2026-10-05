//! The keyboard layout through Hyprland: its main keyboard's.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;

use serde_json::Value;

use crate::services::Kick;
use crate::wm::{hypr_socket, hyprctl};

/// The main keyboard's active layout as Hyprland names it ("English (US)", "Russian"); "" elsewhere.
pub async fn keymap() -> String {
    let out = tokio::task::spawn_blocking(|| hyprctl("j/devices")).await.unwrap_or_default();
    let devices: Value = serde_json::from_str(&out).unwrap_or_default();
    let keyboards = devices["keyboards"].as_array().into_iter().flatten();
    let main = keyboards.into_iter().find(|k| k["main"].as_bool() == Some(true));
    main.and_then(|k| k["active_keymap"].as_str()).unwrap_or("").into()
}

/// A kick on every layout switch, Hyprland's activelayout event; nothing elsewhere.
/// Hyprland's event socket is read on a thread of its own, the runtime having no sockets of its own.
pub async fn events(kick: Kick) {
    let Some(sock) = hypr_socket(".socket2.sock") else { return };
    let _ = tokio::task::spawn_blocking(move || {
        let Ok(c) = UnixStream::connect(sock) else { return };
        for line in BufReader::new(c).lines() {
            let Ok(line) = line else { return };
            if line.starts_with("activelayout>>") && kick.send_blocking(()).is_err() {
                return;
            }
        }
    })
    .await;
}
