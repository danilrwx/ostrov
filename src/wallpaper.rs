//! The wallpaper, in place of swaybg: a layer under everything, the picture covering the screen, black while the
//! wallpaper is off. It follows the pick (modules/wallpaper: ~/.local/state/ostrov/wallpaper.json) as it is
//! kept, a new one fading in over the last. The picture is read off GTK's thread, a full-screen JPEG taking a
//! while.

use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::modules::wallpaper::service as pick_;

/// The picture picked, None while off (or with none).
fn pick() -> Option<std::path::PathBuf> {
    let p = pick_::pick();
    let path = std::path::PathBuf::from(p.path);
    (p.on && path.is_file()).then_some(path)
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
        let path = want.as_ref().map_or(String::new(), |p| p.to_string_lossy().into_owned());
        crate::events::emit("wallpaper", serde_json::json!({"on": want.is_some(), "path": path}));
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
    let file = pick_::file();
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mon) = gio::File::for_path(&file).monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
        mon.connect_changed(move |_, _, _, _| show());
        // kept for the program's life
        std::mem::forget(mon);
    }
}
