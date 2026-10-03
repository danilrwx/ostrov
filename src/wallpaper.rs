//! The wallpaper, in place of swaybg: a layer under everything, the picture covering the screen, black under the
//! dark theme. It follows bin/theme's pick as bin/theme writes it (~/.cache/theme: mode, wallpaper), a new one
//! fading in over the last. The picture is read off GTK's thread, a full-screen JPEG taking a while.

use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::hub::home;

/// What bin/theme picked: the picture, None under the dark theme (or with none).
fn pick() -> Option<std::path::PathBuf> {
    let dir = home().join(".cache/theme");
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap_or_default().trim().to_string();
    if read("mode") == "dark" {
        return None;
    }
    let p = std::path::PathBuf::from(read("wallpaper"));
    p.is_file().then_some(p)
}

pub fn start(app: &gtk4::Application) {
    let win = gtk4::ApplicationWindow::new(app);
    win.init_layer_shell();
    win.set_layer(Layer::Background);
    win.set_namespace(Some("ostrov-wallpaper"));
    for e in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        win.set_anchor(e, true);
    }
    win.set_exclusive_zone(-1);
    win.set_keyboard_mode(KeyboardMode::None);
    win.add_css_class("wallpaper");
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_transition_duration(200);
    win.set_child(Some(&stack));
    win.present();

    let shown = std::rc::Rc::new(std::cell::RefCell::new(None::<std::path::PathBuf>));
    let show = move || {
        let want = pick();
        if *shown.borrow() == want {
            return;
        }
        *shown.borrow_mut() = want.clone();
        let stack = stack.clone();
        let (tx, rx) = async_channel::bounded::<Option<gdk::Texture>>(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(want.and_then(|p| gdk::Texture::from_filename(p).ok()));
        });
        glib::spawn_future_local(async move {
            let Ok(tex) = rx.recv().await else { return };
            let pic = gtk4::Picture::new();
            pic.set_content_fit(gtk4::ContentFit::Cover);
            pic.set_paintable(tex.as_ref());
            stack.add_child(&pic);
            stack.set_visible_child(&pic);
            // the last picture gone once this one has faded in over it
            let s = stack.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
                let mut c = s.first_child();
                while let Some(w) = c {
                    c = w.next_sibling();
                    if w != pic {
                        s.remove(&w);
                    }
                }
            });
        });
    };
    show();
    if let Ok(mon) = gio::File::for_path(home().join(".cache/theme")).monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
        mon.connect_changed(move |_, f, _, _| {
            if matches!(f.basename().as_deref().and_then(|b| b.to_str()), Some("mode" | "wallpaper")) {
                show();
            }
        });
        // kept for the program's life
        std::mem::forget(mon);
    }
}
