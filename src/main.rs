//! ostrov ("island" in Russian): the desktop's shell, everything in one place, in Rust on GTK4 and
//! gtk4-layer-shell. The bar, made of blocks (bar/), with the popups grown out of them in its own window
//! (popup.rs): the quick settings, the calendar, the tray's menus; the launcher in the bar, notifications, the
//! lock screen; the desktop's state and switches from its services (services/, wmd's port), the compositor's
//! through wm.rs, how it all looks in style.rs. One ostrov runs: `ostrov ARGS` hands ARGS to it.

mod bar;
mod calendar;
mod hub;
mod idle;
mod launcher;
mod lock;
mod notes;
mod panel;
mod polkit;
mod popup;
mod prompt;
mod services;
mod style;
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

const USAGE: &str = "usage: ostrov [panel | menu NAME | calendar | run | clip | lock | bar toggle|peek|unpeek | state | dump | \
BLOCK ARGS | SERVICE ARGS]";

/// The bar's blocks, left, middle and right.
const LAYOUT: [&[&str]; 3] = [&["workspaces"], &["clock"], &["layout", "tray", "status"]];

fn activate(app: &gtk4::Application) {
    // the bar's window, the popups laid over it (popup.rs); the bar and the launcher over it its strip
    let over = gtk4::Overlay::new();
    let host = popup::Host::new(app, &over);
    style::load();
    wallpaper::start(app);

    let hub = Hub::start();
    let notes = notes::start(app);
    let bar = bar::Bar::build(&host, &hub, &notes, LAYOUT);
    over.set_child(Some(&bar.strip));

    // the launcher, over the bar between the left's blocks and the right's, the middle's hidden under it
    let place: Rc<std::cell::OnceCell<gtk4::Box>> = Rc::default();
    let launcher = {
        let (h, b, place) = (host.clone(), bar.clone(), place.clone());
        launcher::Launcher::new(move |open| {
            if let Some(w) = place.get().filter(|_| open) {
                let (start, end) = b.middle();
                w.set_margin_start(start + 12);
                w.set_margin_end(end + 12);
            }
            b.hide_middle(open);
            h.launching.set(open);
            h.apply();
        })
    };
    let _ = place.set(launcher.widget.clone());
    over.add_overlay(&launcher.widget);
    let lock = lock::build(app);
    idle::start(&lock);
    let prompts = prompt::Prompts::new(app);
    polkit::start(&prompts);
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
                ["clip"] => launcher.toggle(true),
                ["lock"] => lock.lock(),
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
                    return Ok(words.join(" "));
                }
                [block, ref rest @ ..] if bar.command(block, rest).is_some() => {
                    return bar.command(block, rest).unwrap_or(Ok(String::new()));
                }
                // the services' commands, wmd's words: their outcome said by the running ostrov
                ["wifi" | "bt" | "headset" | "power" | "brightness" | "night" | "location" | "media", ..] => {
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
    // one ostrov: run again, it hands its arguments to the running one and exits
    let app = gtk4::Application::builder()
        .application_id("dev.danil.ostrov")
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
