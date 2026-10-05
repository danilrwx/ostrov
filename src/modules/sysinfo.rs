//! What the computer is doing: the processor's load, the memory in use, the processor's temperature and the
//! network's speed down and up, read from /proc and /sys every two seconds while the widget is; its badge the load
//! and the memory.

use std::time::Instant;

use gtk4::glib;
use gtk4::prelude::*;

use super::{widget, Module};
use crate::cc::{Ctx, Face, Widget};
use crate::i18n::t;
use crate::style::label;

pub const MODULE: Module = Module {
    id: "sysinfo",
    widgets: &[widget("sysinfo", "System", "computer-symbolic", &[(4, 1), (8, 1), (2, 1)], sysinfo)],
    ..Module::NONE
};

/// The processor's time so far, (idle, all), from /proc/stat's first line: idle counting iowait.
fn cpu_times(stat: &str) -> Option<(u64, u64)> {
    let f: Vec<u64> = stat.lines().next()?.split_whitespace().skip(1).take(8).filter_map(|n| n.parse().ok()).collect();
    (f.len() >= 5).then(|| (f[3] + f[4], f.iter().sum()))
}

/// The memory in use and all of it, in bytes, from /proc/meminfo: what is not available counts as used.
fn memory(meminfo: &str) -> Option<(u64, u64)> {
    let kb = |key: &str| {
        let line = meminfo.lines().find(|l| l.starts_with(key))?;
        line.split_whitespace().nth(1)?.parse::<u64>().ok().map(|k| k * 1024)
    };
    let (total, avail) = (kb("MemTotal:")?, kb("MemAvailable:")?);
    Some((total.saturating_sub(avail), total))
}

/// The bytes received and sent so far by the given interfaces, from /proc/net/dev.
fn net_bytes(dev: &str, counted: impl Fn(&str) -> bool) -> (u64, u64) {
    dev.lines()
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            let f: Vec<u64> = rest.split_whitespace().filter_map(|n| n.parse().ok()).collect();
            (counted(name.trim()) && f.len() >= 9).then(|| (f[0], f[8]))
        })
        .fold((0, 0), |(r, s), (a, b)| (r + a, s + b))
}

/// A real device's interface, not lo, a bridge, a VPN's tun (counted again in the device it goes through).
fn physical(name: &str) -> bool {
    std::path::Path::new("/sys/class/net").join(name).join("device").exists()
}

/// The processor's temperature sensor: Intel's coretemp, AMD's k10temp or zenpower, else ACPI's zone.
fn sensor() -> Option<std::path::PathBuf> {
    let hw: Vec<_> = std::fs::read_dir("/sys/class/hwmon").ok()?.flatten().map(|e| e.path()).collect();
    let name = |p: &std::path::PathBuf| std::fs::read_to_string(p.join("name")).unwrap_or_default().trim().to_string();
    ["coretemp", "k10temp", "zenpower", "acpitz"]
        .iter()
        .find_map(|want| hw.iter().find(|p| name(p).starts_with(want)))
        .map(|p| p.join("temp1_input"))
}

/// Bytes as the bar has room for: 512K, 1.2M, 3.4G (binary units, one decimal under 10).
fn size(b: f64) -> String {
    let (n, unit) = [("K", 1024f64), ("M", 1024f64.powi(2)), ("G", 1024f64.powi(3)), ("T", 1024f64.powi(4))]
        .iter()
        .rev()
        .find(|(_, u)| b >= *u)
        .map_or((b, ""), |(n, u)| (b / u, *n));
    if unit.is_empty() { format!("{b:.0}") } else if n < 10.0 { format!("{n:.1}{unit}") } else { format!("{n:.0}{unit}") }
}

/// A value under its caption's word, the word dim.
fn item(caption: &str) -> (gtk4::Box, gtk4::Label) {
    let bx = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let value = label("", "bold");
    bx.append(&label(caption, "dim"));
    bx.append(&value);
    (bx, value)
}

fn sysinfo(_: &Ctx) -> Widget {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
    row.add_css_class("sysinfo");
    row.set_valign(gtk4::Align::Center);
    let (cpu_box, cpu) = item(t("CPU"));
    let (mem_box, mem) = item(t("RAM"));
    let (temp_box, temp) = item(t("Temp"));
    let (net_box, net) = item(t("Net"));
    for b in [&cpu_box, &mem_box, &temp_box, &net_box] {
        row.append(b);
    }
    let badge = gtk4::Label::new(None);
    let sensor = sensor();
    temp_box.set_visible(sensor.is_some());
    let mut last_cpu = None::<(u64, u64)>;
    let mut last_net = None::<(u64, u64, Instant)>;
    // ticking while the widget is: its labels let go of, the tick stops
    let (c, m, tp, n, b) = (cpu.downgrade(), mem.downgrade(), temp.downgrade(), net.downgrade(), badge.downgrade());
    let mut tick = move || {
        let (Some(cpu), Some(mem), Some(temp), Some(net), Some(badge)) = (c.upgrade(), m.upgrade(), tp.upgrade(), n.upgrade(), b.upgrade())
        else {
            return glib::ControlFlow::Break;
        };
        let load = std::fs::read_to_string("/proc/stat").ok().as_deref().and_then(cpu_times).and_then(|now| {
            let was = last_cpu.replace(now)?;
            let all = now.1.saturating_sub(was.1);
            (all > 0).then(|| 100 * all.saturating_sub(now.0.saturating_sub(was.0)) / all)
        });
        let load = load.map_or("…".into(), |p| format!("{p}%"));
        cpu.set_text(&load);
        let used = std::fs::read_to_string("/proc/meminfo").ok().as_deref().and_then(memory);
        let used = used.map_or("…".into(), |(u, _)| size(u as f64));
        mem.set_text(&used);
        badge.set_text(&format!("{load} {used}"));
        if let Some(c) = sensor.as_ref().and_then(|p| std::fs::read_to_string(p).ok()?.trim().parse::<i64>().ok()) {
            temp.set_text(&format!("{}°", c / 1000));
        }
        let now = Instant::now();
        let (rx, tx) = net_bytes(&std::fs::read_to_string("/proc/net/dev").unwrap_or_default(), physical);
        if let Some((r0, t0, at)) = last_net.replace((rx, tx, now)) {
            let s = now.duration_since(at).as_secs_f64().max(0.1);
            net.set_text(&format!("↓{} ↑{}", size(rx.saturating_sub(r0) as f64 / s), size(tx.saturating_sub(t0) as f64 / s)));
        }
        glib::ControlFlow::Continue
    };
    tick();
    glib::timeout_add_seconds_local(2, tick);
    let has_temp = temp_box.is_visible();
    Widget {
        face: Some(Face::new(&badge)),
        // two cells the load and the memory, four the temperature too, six the network as well
        size: Box::new(move |w, _| {
            temp_box.set_visible(has_temp && w >= 4);
            net_box.set_visible(w >= 6);
        }),
        ..Widget::new(&row, None, |_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_read() {
        assert_eq!(cpu_times("cpu  10 0 5 80 5 0 0 0 0 0\ncpu0 1 2 3 4"), Some((85, 100)));
        let mi = "MemTotal:       16000 kB\nMemFree:  100 kB\nMemAvailable:   6000 kB\n";
        assert_eq!(memory(mi), Some((10000 * 1024, 16000 * 1024)));
        let dev = "Inter-|\n face |\n    lo: 50 1 0 0 0 0 0 0 50 1 0 0 0 0 0 0\n wlan0: 1000 9 0 0 0 0 0 0 200 3 0 0 0 0 0 0\n";
        assert_eq!(net_bytes(dev, |n| n != "lo"), (1000, 200));
        assert_eq!(size(512.0), "512");
        assert_eq!(size(1536.0), "1.5K");
        assert_eq!(size(5.5 * 1024f64.powi(3)), "5.5G");
        assert_eq!(size(20.0 * 1024f64.powi(2)), "20M");
    }
}
