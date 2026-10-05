//! The keyboard layout through Hyprland: its main keyboard's.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;

use serde_json::{json, Value};

use crate::services::Kick;
use crate::wm::{hypr_socket, hyprctl};

/// The main keyboard's layouts as Hyprland has them: their xkb codes in order (us, ru), the active one's index and
/// its name ("English (US)", "Russian"); {} elsewhere.
pub async fn keymap() -> Value {
    let out = tokio::task::spawn_blocking(|| hyprctl("j/devices")).await.unwrap_or_default();
    let devices: Value = serde_json::from_str(&out).unwrap_or_default();
    let keyboards = devices["keyboards"].as_array().into_iter().flatten();
    let Some(main) = keyboards.into_iter().find(|k| k["main"].as_bool() == Some(true)) else { return json!({}) };
    let layouts: Vec<&str> = main["layout"].as_str().unwrap_or("").split(',').map(str::trim).filter(|l| !l.is_empty()).collect();
    json!({"name": main["active_keymap"], "index": main["active_layout_index"], "layouts": layouts})
}

/// `ostrov keymap next|set N`: the main keyboard's next layout, or the Nth (0 the first).
pub async fn cmd(args: &[&str]) -> crate::services::Res {
    let req = match args {
        ["next"] => "switchxkblayout main next".to_string(),
        ["set", n] => format!("switchxkblayout main {}", n.parse::<u32>().map_err(|_| format!("not a layout's number: {n}"))?),
        _ => return Err(super::MODULE.usage()),
    };
    let out = tokio::task::spawn_blocking(move || hyprctl(&req)).await.unwrap_or_default();
    if out.trim() == "ok" { Ok(()) } else { Err(out.trim().to_string()) }
}

/// A layout's xkb code as the widget shows it: a flag (its country's, us 🇺🇸), its language in two letters (us EN,
/// ua UK), or the code as it is.
pub fn shown(code: &str, how: &str) -> String {
    let code = code.to_lowercase();
    match how {
        "flag" => {
            let country = match code.as_str() {
                "en" => "gb",
                "latam" => "es",
                c => c,
            };
            let letters: Vec<char> = country.chars().filter(char::is_ascii_lowercase).take(2).collect();
            if letters.len() < 2 {
                return code.to_uppercase();
            }
            letters.iter().filter_map(|c| char::from_u32(0x1F1E6 + (*c as u32 - 'a' as u32))).collect()
        }
        "language" => {
            let lang = match code.as_str() {
                "us" | "gb" | "au" | "ca" | "nz" | "ie" | "za" => "en",
                "ua" => "uk",
                "by" => "be",
                "cz" => "cs",
                "se" => "sv",
                "dk" => "da",
                "jp" => "ja",
                "cn" => "zh",
                "kr" => "ko",
                "il" => "he",
                "gr" => "el",
                "br" => "pt",
                "latam" => "es",
                "ee" => "et",
                "kz" => "kk",
                "ge" => "ka",
                "am" => "hy",
                c => c,
            };
            lang.to_uppercase()
        }
        _ => code,
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn layouts_shown() {
        use super::shown;
        assert_eq!(shown("us", "flag"), "🇺🇸");
        assert_eq!(shown("ru", "flag"), "🇷🇺");
        assert_eq!(shown("us", "language"), "EN");
        assert_eq!(shown("ru", "language"), "RU");
        assert_eq!(shown("ua", "language"), "UK");
        assert_eq!(shown("de", "code"), "de");
    }

    #[tokio::test]
    async fn keymap_state() {
        println!("keymap {}", serde_json::json!(super::keymap().await));
    }
}
