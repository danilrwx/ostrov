//! The keyboard layout through the compositor (hypr.go): Hyprland's main keyboard, sway's first.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;

use serde_json::Value;
use tokio::io::AsyncBufReadExt;

use crate::services::Kick;
use crate::wm::{hypr_socket, hyprctl};

fn under_sway() -> bool {
    std::env::var_os("SWAYSOCK").is_some_and(|s| !s.is_empty())
}

/// The main keyboard's active layout as the compositor names it ("English (US)", "Russian"): Hyprland's main
/// keyboard, sway's first one; "" under neither.
pub async fn keymap() -> String {
    if under_sway() {
        return sway_keymap().await;
    }
    let out = tokio::task::spawn_blocking(|| hyprctl("j/devices")).await.unwrap_or_default();
    let devices: Value = serde_json::from_str(&out).unwrap_or_default();
    let keyboards = devices["keyboards"].as_array().into_iter().flatten();
    let main = keyboards.into_iter().find(|k| k["main"].as_bool() == Some(true));
    main.and_then(|k| k["active_keymap"].as_str()).unwrap_or("").into()
}

/// A kick on every layout switch, Hyprland's activelayout event or sway's input one; nothing under neither.
/// Hyprland's event socket is read on a thread of its own, the runtime having no sockets of its own.
pub async fn events(kick: Kick) {
    if under_sway() {
        return sway_events(kick).await;
    }
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

/// The first keyboard's active layout under sway.
async fn sway_keymap() -> String {
    let Ok(out) = tokio::process::Command::new("swaymsg").args(["-r", "-t", "get_inputs"]).output().await else {
        return String::new();
    };
    if !out.status.success() {
        return String::new();
    }
    let inputs: Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let mut keyboards = inputs.as_array().into_iter().flatten().filter(|i| i["type"] == "keyboard");
    let layout = keyboards.find_map(|i| i["xkb_active_layout_name"].as_str().filter(|l| !l.is_empty()));
    layout.unwrap_or("").into()
}

/// A kick on each line of sway's input events (a layout switch among them), for as long as swaymsg runs.
async fn sway_events(kick: Kick) {
    let Ok(mut child) = tokio::process::Command::new("swaymsg")
        .args(["-m", "-t", "subscribe", r#"["input"]"#])
        .stdout(std::process::Stdio::piped())
        .spawn()
    else {
        return;
    };
    let Some(out) = child.stdout.take() else { return };
    let mut lines = tokio::io::BufReader::new(out).lines();
    while let Ok(Some(_)) = lines.next_line().await {
        if kick.send(()).await.is_err() {
            break;
        }
    }
    let _ = child.wait().await;
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn keymap_state() {
        println!("keymap {}", serde_json::json!(super::keymap().await));
    }
}
