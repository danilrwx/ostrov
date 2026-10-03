//! ostrov ("island" in Russian): the desktop's shell, everything in one place, in Rust on GTK4 and
//! gtk4-layer-shell. The bar, made of blocks (bar/), with the popups grown out of them in its own window
//! (popup.rs): the quick settings, the calendar, the tray's menus; the launcher in the bar, notifications, the
//! lock screen; the desktop's state and switches from its services (services/, wmd's port), the compositor's
//! through wm.rs, how it all looks in style.rs. One ostrov runs: `ostrov ARGS` hands ARGS to it.

mod bar;
mod calc;
mod calendar;
mod clip;
mod config;
mod greet;
mod hub;
mod idle;
mod keys;
mod launcher;
mod lock;
mod notes;
mod panel;
mod polkit;
mod popup;
mod prompt;
mod record;
mod services;
mod shot;
mod style;
mod switcher;
mod wallpaper;
mod wm;

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use hub::Hub;

thread_local! {
    /// What `ostrov ARGS` does in the running ostrov, set once it is built.
    static COMMAND: std::cell::RefCell<Option<Box<dyn Fn(&[String]) -> Result<String, String>>>> = Default::default();
    /// The questions ostrov puts (polkit's, askpass's), set once it is built.
    static PROMPTS: std::cell::RefCell<Option<Rc<prompt::Prompts>>> = Default::default();
}

const USAGE: &str = "usage: ostrov [panel | menu NAME | calendar | run | ask QUESTION | clip | windows [app] | lock | key NAME | awake | screenshot | \
capture FILE | record [--audio] | bar toggle|peek|unpeek | state | dump | BLOCK ARGS | SERVICE ARGS]";

fn activate(app: &gtk4::Application) {
    // the bar's window, the popups laid over it (popup.rs); the bar and the launcher over it its strip
    let over = gtk4::Overlay::new();
    let host = popup::Host::new(app, &over);
    style::load();
    wallpaper::start(app);
    clip::start();

    let hub = Hub::start();
    let notes = notes::start(app);
    let cfg = config::load();
    fn names(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }
    let (l, c, r) = (names(&cfg.bar.left), names(&cfg.bar.center), names(&cfg.bar.right));
    let bar = bar::Bar::build(&host, &hub, &notes, [&l, &c, &r]);
    over.set_child(Some(&bar.strip));

    // the launcher, over the bar between the left's blocks and the right's, the middle's hidden under it; the
    // clipboard entry picked shown under it
    let preview = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    preview.set_halign(gtk4::Align::Start);
    preview.set_valign(gtk4::Align::Start);
    preview.set_can_target(false);
    let place: Rc<std::cell::OnceCell<gtk4::Box>> = Rc::default();
    let launcher = {
        let (h, b, place) = (host.clone(), bar.clone(), place.clone());
        let pv = preview.clone();
        launcher::Launcher::new(move |open| {
            if let Some(w) = place.get().filter(|_| open) {
                let (start, end) = b.middle();
                w.set_margin_start(start + 12);
                w.set_margin_end(end + 12);
                pv.set_margin_start(start + 12);
            }
            b.hide_middle(open);
            h.launching.set(open);
            h.apply();
        })
    };
    let _ = place.set(launcher.widget.clone());
    preview.append(&launcher.preview);
    host.overlay(&preview);
    over.add_overlay(&launcher.widget);
    let lock = lock::build(app);
    idle::start(&lock, &cfg.idle);
    let shot = shot::Shot::new(app);
    let switcher = switcher::Switcher::new(app);
    record::init(app, &shot);
    let prompts = prompt::Prompts::new(app);
    polkit::start(&prompts);
    keys::battery(&hub, &notes, &prompts);
    let keys = keys::Keys::new(&hub, &notes);
    PROMPTS.with(|p| *p.borrow_mut() = Some(prompts));

    // ostrov ARGS, from a key or a script, handed over to this ostrov by GApplication
    COMMAND.with(|c| {
        *c.borrow_mut() = Some(Box::new(move |args: &[String]| {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let toggle = |name: &str| bar.popup(name).map(|p| p.toggle()).ok_or(format!("no block {name}"));
            match args[..] {
                ["panel"] => toggle("status")?,
                ["calendar"] => toggle("clock")?,
                ["menu", name] => return bar.command("status", &["menu", name]).unwrap_or(Err("no status block".into())),
                ["run"] => launcher.toggle(false),
                ["ask", ref question @ ..] if !question.is_empty() => {
                    let ctx = gtk4::gdk::Display::default().map(|d| d.app_launch_context());
                    let uri = launcher::claude(&question.join(" "));
                    gtk4::gio::AppInfo::launch_default_for_uri(&uri, ctx.as_ref()).map_err(|e| e.to_string())?;
                }
                ["clip"] => launcher.toggle(true),
                ["lock"] => lock.lock(),
                ["screenshot"] => shot.take(),
                ["awake"] => idle::set_awake(!idle::awake()),
                ["capture", path] => shot::capture(path.to_string()),
                ["windows"] => switcher.open(false),
                ["windows", "app"] => switcher.open(true),
                ["windows", "release"] => switcher.release(),
                ["record"] => record::toggle(false),
                ["record", "--audio"] => record::toggle(true),
                ["key", name] => keys.key(name)?,
                ["bar", what @ ("toggle" | "peek" | "unpeek")] => {
                    match what {
                        "toggle" => host.docked.set(!host.docked.get()),
                        "peek" => host.peeking.set(true),
                        _ => host.peeking.set(false),
                    }
                    host.apply();
                }
                // the services' state, wmd watch's JSON
                ["dump"] => return Ok(hub.state().to_string()),
                // what is open, and the bar's mode: for a script, a test
                ["state"] => {
                    let mut words: Vec<&str> = bar.open().into_iter().collect();
                    words.push(if host.docked.get() { "docked" } else { "hidden" });
                    if launcher.is_open() {
                        words.push("launcher");
                    }
                    if idle::awake() {
                        words.push("awake");
                    }
                    return Ok(words.join(" "));
                }
                [block, ref rest @ ..] if bar.command(block, rest).is_some() => {
                    return bar.command(block, rest).unwrap_or(Ok(String::new()));
                }
                // the services' commands, wmd's words: their outcome said by the running ostrov
                ["wifi" | "bt" | "headset" | "audio" | "power" | "brightness" | "night" | "location" | "media" | "displays"
                | "calendar", ..] => {
                    hub::service(&args)
                }
                _ => return Err(USAGE.into()),
            }
            Ok(String::new())
        }))
    });
}

/// ssh's askpass (bin/askpass, SSH_ASKPASS): `ostrov askpass [--confirm|--none] PROMPT`, a key's passphrase
/// asked and printed, or a yes or no to using a key (--confirm: the exit status says it; --none: a word alone).
/// The asking ostrov waits for the answer: the command line kept until it comes, its status set then.
fn askpass(cl: &gtk4::gio::ApplicationCommandLine, args: &[String]) {
    let (secret, text) = match args {
        [flag, rest @ ..] if flag == "--confirm" || flag == "--none" => (false, rest.join(" ")),
        rest => (true, rest.join(" ")),
    };
    let Some(prompts) = PROMPTS.with(|p| p.borrow().clone()) else { return };
    let (reply, answer) = async_channel::bounded(1);
    let (title, text) = match text.split_once('\n') {
        Some((t, rest)) => (t.trim().to_string(), rest.trim().to_string()),
        None => (text.trim().to_string(), String::new()),
    };
    prompts.ask(prompt::Ask { icon: "dialog-password-symbolic".into(), title, text, secret, error: String::new(), reply });
    let cl = cl.clone();
    glib::spawn_future_local(async move {
        match answer.recv().await.ok().flatten() {
            Some(p) => {
                if secret {
                    cl.print_literal(&format!("{p}\n"));
                }
                cl.set_exit_code(glib::ExitCode::SUCCESS);
            }
            None => cl.set_exit_code(glib::ExitCode::FAILURE),
        }
        cl.done();
    });
}

fn main() -> glib::ExitCode {
    // the login screen, greetd's greeter (greet.rs): none of the desktop, nor the one ostrov
    if std::env::args().nth(1).as_deref() == Some("greet") {
        return greet::run();
    }
    // one ostrov: run again, it hands its arguments to the running one and exits
    // OSTROV_APP_ID: another id, a second ostrov beside the running one (a build tried out without stopping it)
    let id = std::env::var("OSTROV_APP_ID").unwrap_or_else(|_| "dev.danil.ostrov".into());
    let app = gtk4::Application::builder()
        .application_id(id.as_str())
        .flags(gtk4::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    app.connect_activate(activate);
    app.connect_command_line(|app, cl| {
        if COMMAND.with(|c| c.borrow().is_none()) {
            app.activate();
        }
        let args: Vec<String> = cl.arguments().iter().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
        if args.is_empty() {
            return glib::ExitCode::SUCCESS;
        }
        if args[0] == "askpass" {
            askpass(cl, &args[1..]);
            return glib::ExitCode::FAILURE;
        }
        match COMMAND.with(|c| c.borrow().as_ref().map(|f| f(&args))) {
            Some(Err(e)) => {
                cl.printerr_literal(&format!("{e}\n"));
                glib::ExitCode::FAILURE
            }
            Some(Ok(out)) if !out.is_empty() => {
                cl.print_literal(&format!("{out}\n"));
                glib::ExitCode::SUCCESS
            }
            _ => glib::ExitCode::SUCCESS,
        }
    });
    app.run()
}
