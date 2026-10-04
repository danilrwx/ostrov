//! The screen-share picker: what xdg-desktop-portal-hyprland asks when an app wants the screen, in place of its Qt
//! hyprland-share-picker, in ostrov's dialog (prompt.rs): a screen, a window, or a region dragged out with the
//! screenshot's selector (shot.rs), and whether the app may share it again without asking. xdph runs the picker
//! itself, no shell between (its screencopy:custom_picker_binary, a path and no arguments: ostrov run as
//! ostrov-share-picker, main.rs), --allow-token on it when its allow_token_by_default is set, its windows in
//! XDPH_WINDOW_SHARING_LIST; it reads "[SELECTION]FLAGS/WHAT" from the picker's stdout (r in FLAGS: a restore token
//! allowed; WHAT screen:OUTPUT, window:HANDLE, region:OUTPUT@X,Y,W,H in the output's logical pixels from its
//! corner), nothing there nothing shared, the exit status never looked at.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{gdk, glib, Align, Orientation};

use crate::i18n::t;
use crate::prompt::{Ask, Kind};
use crate::style::label;

/// A window xdph can share: its handle (the toplevel's, what window:HANDLE names), its app's class, its title.
#[derive(Debug, PartialEq)]
pub struct Window {
    pub handle: u32,
    pub class: String,
    pub title: String,
}

/// XDPH_WINDOW_SHARING_LIST: HANDLE[HC>]CLASS[HT>]TITLE[HE>]ADDRESS[HA>] a window, one after another (xdph keeps
/// ">]" out of the names).
pub fn windows(list: &str) -> Vec<Window> {
    let read = |w: &str| {
        let (handle, rest) = w.split_once("[HC>]")?;
        let (class, rest) = rest.split_once("[HT>]")?;
        let (title, _) = rest.split_once("[HE>]")?;
        Some(Window { handle: handle.parse().ok()?, class: class.into(), title: title.into() })
    };
    list.split("[HA>]").filter_map(read).collect()
}

/// The line xdph reads: what is shared, and whether the app may share it again without asking.
fn selection(remember: bool, what: &str) -> String {
    format!("[SELECTION]{}/{what}", if remember { "r" } else { "" })
}

/// A region as pick-region says it ("X,Y WxH", the desktop's logical coordinates) as xdph takes it: on the output
/// (name, x, y, width, height on the desktop) its corner is on, from that output's corner.
fn region(picked: &str, outputs: &[(String, i32, i32, i32, i32)]) -> Option<String> {
    let (xy, wh) = picked.split_once(' ')?;
    let ((x, y), (w, h)) = (xy.split_once(',')?, wh.split_once('x')?);
    let [x, y, w, h] = [x, y, w, h].map(|v| v.trim().parse::<i32>().ok());
    let (x, y, w, h) = (x?, y?, w?, h?);
    let (name, ox, oy, ..) =
        outputs.iter().find(|(_, ox, oy, ow, oh)| (*ox..ox + ow).contains(&x) && (*oy..oy + oh).contains(&y))?;
    Some(format!("region:{name}@{},{},{w},{h}", x - ox, y - oy))
}

fn monitors() -> Vec<gdk::Monitor> {
    let Some(d) = gdk::Display::default() else { return Vec::new() };
    d.monitors().iter::<gdk::Monitor>().filter_map(Result::ok).collect()
}

/// `share-pick LIST [--allow-token]` in the running ostrov (main.rs hands it xdph's list): the dialog, then the
/// line xdph reads; an error for a no, nothing on stdout.
pub async fn pick(args: Vec<String>, shot: Option<Rc<crate::shot::Shot>>) -> Result<String, String> {
    let prompts = crate::prompts().ok_or("ostrov is starting")?;
    let list = args.first().filter(|a| *a != "--allow-token").cloned().unwrap_or_default();
    let remember = args.iter().any(|a| a == "--allow-token");
    let (reply, answer) = async_channel::bounded(1);
    let text = t("An app asks to see your screen. Pick what it sees.");
    let mut a = Ask::new("video-display-symbolic", t("Share the screen"), text, Kind::Share { list, remember }, reply);
    a.ok = t("Share").into();
    prompts.ask(a);
    let picked = answer.recv().await.ok().flatten().ok_or("cancelled")?;
    let Some(head) = picked.strip_suffix("region") else { return Ok(picked) };
    // the dialog off the screen before the screen is taken for the selector
    glib::timeout_future(Duration::from_millis(200)).await;
    let dragged = shot.ok_or("ostrov is starting")?.pick().await?;
    let outputs: Vec<_> = monitors()
        .iter()
        .map(|m| {
            let g = m.geometry();
            (m.connector().unwrap_or_default().to_string(), g.x(), g.y(), g.width(), g.height())
        })
        .collect();
    let at = region(&dragged, &outputs).ok_or(format!("no output under the region {dragged}"))?;
    Ok(format!("{head}{at}"))
}

/// The dialog's body for xdph's question: Screen, Window and Region, one at a time, a card each to pick in the
/// first two, and the switch to remember the choice. Its answer the line xdph reads, a region's ending in "region"
/// (dragged out once the dialog is down). Share is off with nothing to pick. The pages as tall as the tallest, the
/// segments kept in place as they change.
pub fn body(body: &gtk4::Box, list: &str, remember: bool, ok: &gtk4::Button) -> Box<dyn Fn() -> String> {
    let page = Rc::new(Cell::new(0));
    let picked: Rc<RefCell<[String; 2]>> = Rc::default();
    let stack = gtk4::Stack::new();
    stack.set_margin_top(8);

    // a page's cards, the first picked; a click on one picks it
    let cards = |n: usize, items: Vec<(gtk4::Image, String, String, String)>| {
        let bx = gtk4::Box::new(Orientation::Vertical, 6);
        let mut first: Option<gtk4::ToggleButton> = None;
        for (icon, name, note, what) in items {
            let b = gtk4::ToggleButton::new();
            b.add_css_class("chip");
            match &first {
                Some(f) => b.set_group(Some(f)),
                None => {
                    b.set_active(true);
                    picked.borrow_mut()[n] = what.clone();
                    first = Some(b.clone());
                }
            }
            let line = gtk4::Box::new(Orientation::Horizontal, 10);
            icon.set_pixel_size(24);
            line.append(&icon);
            let words = gtk4::Box::new(Orientation::Vertical, 2);
            words.set_valign(Align::Center);
            for (text, class) in [(name, ""), (note, "dim")] {
                let l = label(&text, class);
                l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                l.set_max_width_chars(44);
                words.append(&l);
            }
            line.append(&words);
            b.set_child(Some(&line));
            let p = picked.clone();
            b.connect_toggled(move |b| {
                if b.is_active() {
                    p.borrow_mut()[n] = what.clone();
                }
            });
            bx.append(&b);
        }
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_hscrollbar_policy(gtk4::PolicyType::Never);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(320);
        scroll.set_child(Some(&bx));
        scroll
    };

    let screens = monitors()
        .iter()
        .map(|m| {
            let name = m.connector().unwrap_or_default().to_string();
            let g = m.geometry();
            let px = |v: i32| (v as f64 * m.scale()).round();
            let model = [m.manufacturer(), m.model()].into_iter().flatten().collect::<Vec<_>>().join(" ");
            let size = format!("{}×{}", px(g.width()), px(g.height()));
            let note = if model.is_empty() { size } else { format!("{model} · {size}") };
            (gtk4::Image::from_icon_name("video-display-symbolic"), name.clone(), note, format!("screen:{name}"))
        })
        .collect();
    stack.add_named(&cards(0, screens), Some("0"));

    let windows = windows(list);
    let has_windows = !windows.is_empty();
    if has_windows {
        let items = windows
            .into_iter()
            .map(|w| {
                let icon = match crate::bar::app_icon(&w.class) {
                    Some(g) => gtk4::Image::from_gicon(&g),
                    None => gtk4::Image::from_icon_name("application-x-executable-symbolic"),
                };
                (icon, w.title, w.class, format!("window:{}", w.handle))
            })
            .collect();
        stack.add_named(&cards(1, items), Some("1"));
    } else {
        let none = label(t("No windows to share"), "dim");
        none.set_margin_top(12);
        none.set_margin_bottom(12);
        stack.add_named(&none, Some("1"));
    }

    // the region's page: its button the same as Share
    let o = ok.clone();
    let drag = crate::ui::row("edit-select-all-symbolic", t("Drag out a region…"), "", false, move || {
        o.emit_clicked();
    });
    drag.set_valign(Align::Center);
    stack.add_named(&drag, Some("2"));

    let (s, p, o) = (stack.clone(), page.clone(), ok.clone());
    let segments = crate::ui::options(&[t("Screen"), t("Window"), t("Region")], &[true], false, move |i, _| {
        p.set(i);
        s.set_visible_child_name(&i.to_string());
        o.set_sensitive(i != 1 || has_windows);
    });
    let switch = gtk4::Switch::new();
    switch.set_active(remember);
    let help = t("The app may share it again without asking");
    let keep = crate::ui::setting(t("Remember this choice"), help, &switch, false);
    keep.set_margin_top(8);
    body.append(&segments);
    body.append(&stack);
    body.append(&keep);

    Box::new(move || {
        let what = match page.get() {
            2 => "region".to_string(),
            n => picked.borrow()[n].clone(),
        };
        selection(switch.is_active(), &what)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdph_windows_read() {
        let list = "94[HC>]firefox[HT>]Ostrov — Mozilla Firefox[HE>]93824992283648[HA>]\
                    7[HC>]foot[HT>]~ [HE>]0[HA>]";
        let w = windows(list);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], Window { handle: 94, class: "firefox".into(), title: "Ostrov — Mozilla Firefox".into() });
        assert_eq!((w[1].handle, w[1].title.as_str()), (7, "~ "));
        assert!(windows("").is_empty());
        assert!(windows("x[HC>]a[HT>]b[HE>]0[HA>]").is_empty());
    }

    #[test]
    fn selections_said_as_xdph_reads_them() {
        assert_eq!(selection(true, "screen:DP-1"), "[SELECTION]r/screen:DP-1");
        assert_eq!(selection(false, "window:94"), "[SELECTION]/window:94");
        let outputs = [("eDP-1".to_string(), 0, 0, 1600, 1000), ("DP-1".to_string(), 1600, 0, 2560, 1440)];
        assert_eq!(region("1700,40 300x200", &outputs).as_deref(), Some("region:DP-1@100,40,300,200"));
        assert_eq!(region("0,0 1600x1000", &outputs).as_deref(), Some("region:eDP-1@0,0,1600,1000"));
        assert_eq!(region("5000,0 10x10", &outputs), None);
        assert_eq!(region("garbage", &outputs), None);
        let line = format!("{}{}", selection(true, "region").strip_suffix("region").unwrap(), "region:DP-1@0,0,1,1");
        assert_eq!(line, "[SELECTION]r/region:DP-1@0,0,1,1");
    }
}
