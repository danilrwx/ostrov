//! ostrov ("island" in Russian): the desktop's shell, everything in one place, in Rust on GTK4 and
//! gtk4-layer-shell. The bar, made of blocks (bar/), with the popups grown out of them in its own window
//! (popup.rs): the quick settings, the calendar, the tray's menus; the launcher in the bar, notifications, the
//! lock screen; the desktop's state and switches from its services (services/, wmd's port), the compositor's
//! through wm.rs, how it all looks in style.rs. One ostrov runs: `ostrov ARGS` hands ARGS to it.

mod api;
mod backend;
mod bar;
mod calc;
mod cc;
mod clip;
mod config;
mod doctor;
mod forms;
mod greet;
mod hub;
mod idle;
mod keys;
mod launcher;
mod lock;
mod modules;
mod look;
mod notes;
mod overview;
mod plugins;
mod polkit;
mod popup;
mod prompt;
mod record;
mod services;
mod settings;
mod shot;
mod style;
mod switcher;
mod ui;
mod wallpaper;
mod widgets;
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

/// ostrov's own commands, in forms.rs's grammar; besides them a bar block's (BLOCK ARGS) and a module's (MODULE
/// ARGS).
const FORMS: &[&str] = &[
    "panel", "menu NAME", "settings [SECTION]", "appearance", "calendar", "run", "ask QUESTION...", "clip",
    "windows [app]", "overview [close]", "lock", "restart", "hyprland", "key NAME", "awake", "screenshot", "capture FILE", "record [--audio]",
    "bar toggle|peek|unpeek", "state", "dump", "toast TITLE [BODY...]", "dialog JSON", "plugins",
    "plugin [ID] [ARGS...]", "help", "complete [WORD...]", "completions zsh|bash|fish", "doctor",
];

fn usage() -> String {
    forms::usage("", FORMS) + " | BLOCK ARGS | MODULE ARGS"
}

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
    // the plugins' widgets before the control centre takes them; ostrov's D-Bus face (api.rs)
    plugins::start(&hub);
    api::start(&hub);
    fn names(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }
    let (l, c, r) = (names(&cfg.bar.left), names(&cfg.bar.center), names(&cfg.bar.right));
    let bar = bar::Bar::build(&host, &hub, [&l, &c, &r]);
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
    let overview = overview::Overview::new(app);
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
            let panel = |id: &str| cc::panel(id).ok_or(format!("no panel {id} in the bar"));
            match args[..] {
                ["panel"] => panel("control")?.toggle(),
                ["panel", id] => panel(id)?.toggle(),
                ["calendar"] => panel("calendar")?.toggle(),
                // the panel with that widget on it, the widget's menu unfolded ("edit": the control centre in its
                // editing)
                ["menu", name] => {
                    let key = cc::alias(name);
                    cc::with_widget(key).or_else(|| cc::panel("control")).ok_or("no panel in the bar")?.open_menu(name);
                }
                // the control centre at its Settings (an entry's form: bar, calendar, widget.wallpaper...) or
                // Appearance page
                ["settings", ref entry @ ..] => panel("control")?.open_page("settings", entry.first().copied()),
                ["appearance"] => panel("control")?.open_page("appearance", None),
                ["run"] => launcher.toggle(false),
                ["ask", ref question @ ..] if !question.is_empty() => {
                    let ctx = gtk4::gdk::Display::default().map(|d| d.app_launch_context());
                    let uri = launcher::claude(&question.join(" "));
                    gtk4::gio::AppInfo::launch_default_for_uri(&uri, ctx.as_ref()).map_err(|e| e.to_string())?;
                }
                ["clip"] => launcher.toggle(true),
                ["lock"] => lock.lock(),
                ["restart"] => restart(),
                ["hyprland"] => return Ok(modules::hyprland::conf()),
                ["screenshot"] => shot.take(),
                ["awake"] => idle::set_awake(!idle::awake()),
                ["capture", path] => shot::capture(path.to_string()),
                ["windows"] => switcher.open(false),
                ["windows", "app"] => switcher.open(true),
                ["windows", "release"] => switcher.release(),
                ["overview"] => overview.toggle(),
                ["overview", "close"] => overview.close(),
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
                ["toast", title, ref body @ ..] => {
                    notes.post("dialog-information-symbolic", title, &body.join(" "), false)
                }
                ["plugins"] => return Ok(plugins::list()),
                // the shells' completion (forms.rs), from what this ostrov knows
                ["complete", ref words @ ..] => {
                    let known = forms::Known {
                        state: hub.state(),
                        widgets: cc::menus(),
                        sections: settings::entries().into_iter().map(|e| (e.id, e.title)).collect(),
                        plugins: plugins::known(),
                        blocks: bar.forms(),
                    };
                    return Ok(forms::lines(&forms::complete(&known, if words.is_empty() { &[""] } else { words })));
                }
                ["help"] => {
                    return Ok(format!("{}\n\nthe modules' commands:\n{}\n\nthe plugins' commands:\n{}", usage(), services::usage(), plugins::help()));
                }
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
                [first, ..] if services::is_command(first) => hub::service(&args),
                _ => return Err(format!("{}\n{}", usage(), services::usage())),
            }
            Ok(String::new())
        }))
    });
}

/// A command's outcome, once there is one: a plugin's comes when it answers.
pub type Reply = std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>>>>;

/// `ostrov ARGS` done by this ostrov, whoever asks: the command line, D-Bus (api.rs), a plugin. The one dispatch:
/// `plugin ID ARGS` to that plugin's own (input what was piped to ostrov), `dialog JSON` a question's answer once
/// given (a no an error), the rest ostrov's own, at once.
fn command(args: &[String], input: Option<String>) -> Reply {
    match args {
        [first, rest @ ..] if first == "plugin" => return plugins::run(rest, input),
        [first] if first == "doctor" => return Box::pin(doctor::report()),
        [first, rest @ ..] if first == "dialog" => {
            let spec = rest.join(" ");
            return Box::pin(async move {
                let spec = serde_json::from_str(&spec).map_err(|e| format!("dialog JSON: {e}"))?;
                prompt::dialog(&spec, None, None).await?.ok_or("cancelled".into())
            });
        }
        _ => {}
    }
    let r = COMMAND.with(|c| c.borrow().as_ref().map(|f| f(args))).unwrap_or(Err("ostrov is starting".into()));
    Box::pin(std::future::ready(r))
}

/// ostrov started again in its own place (its pid, its stdout, its environment): the binary at its path now (a
/// new build installed), its config read anew. Locked, it locks again at once (lock.rs).
pub fn restart() {
    use std::os::unix::process::CommandExt;
    let me = std::env::args_os().next().unwrap_or_else(|| "ostrov".into());
    let e = std::process::Command::new(me).exec();
    eprintln!("ostrov: restart: {e}");
}

/// The questions ostrov puts, once built.
pub fn prompts() -> Option<Rc<prompt::Prompts>> {
    PROMPTS.with(|p| p.borrow().clone())
}

/// What was piped to `ostrov ARGS`, read on a thread of its own: nothing from a terminal, nothing empty.
fn piped(cl: &gtk4::gio::ApplicationCommandLine) -> Option<async_channel::Receiver<String>> {
    use std::io::{IsTerminal, Read};
    use std::os::fd::AsFd;
    let stream = cl.stdin()?.downcast::<gio_unix::InputStream>().ok()?;
    let fd = stream.as_fd().try_clone_to_owned().ok().filter(|fd| !fd.is_terminal())?;
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::fs::File::from(fd).read_to_string(&mut text);
        let _ = tx.send_blocking(text);
    });
    Some(rx)
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
    let kind = if secret { prompt::Kind::Secret } else { prompt::Kind::Confirm };
    prompts.ask(prompt::Ask::new("dialog-password-symbolic", &title, &text, kind, reply));
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
    // the shells' completion: the script printed here; its candidates by the running ostrov, nothing with none
    // (never this one made the shell)
    match std::env::args().nth(1).as_deref() {
        Some("completions") => {
            let script = std::env::args().nth(2).and_then(|s| forms::script(&s));
            let Some(script) = script else {
                eprintln!("{}", forms::usage("completions", &["zsh|bash|fish"]));
                return glib::ExitCode::FAILURE;
            };
            print!("{script}");
            return glib::ExitCode::SUCCESS;
        }
        Some("complete") if !forms::running(&id) => return glib::ExitCode::SUCCESS,
        _ => {}
    }
    let app = gtk4::Application::builder()
        .application_id(id.as_str())
        .flags(gtk4::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    app.connect_activate(activate);
    app.connect_command_line(|app, cl| {
        let args: Vec<String> = cl.arguments().iter().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
        if COMMAND.with(|c| c.borrow().is_none()) {
            // a completion that found the ostrov it asked gone by now: answered by nothing here either
            if args.first().is_some_and(|a| a == "complete") {
                return glib::ExitCode::SUCCESS;
            }
            app.activate();
        }
        if args.is_empty() {
            return glib::ExitCode::SUCCESS;
        }
        if args[0] == "askpass" {
            askpass(cl, &args[1..]);
            return glib::ExitCode::FAILURE;
        }
        // a plugin's command reads what was piped (a built-in one never: a script's loop keeps its stdin), and
        // answers once it has: the command line kept till then, its status set then
        let input = if args[0] == "plugin" { piped(cl) } else { None };
        let cl = cl.clone();
        glib::spawn_future_local(async move {
            let input = match input {
                Some(rx) => rx.recv().await.ok().filter(|t| !t.is_empty()),
                None => None,
            };
            match command(&args, input).await {
                Err(e) => {
                    cl.printerr_literal(&format!("{e}\n"));
                    cl.set_exit_code(glib::ExitCode::FAILURE);
                }
                Ok(out) => {
                    if !out.is_empty() {
                        cl.print_literal(&format!("{out}\n"));
                    }
                    cl.set_exit_code(glib::ExitCode::SUCCESS);
                }
            }
            cl.done();
        });
        glib::ExitCode::SUCCESS
    });
    app.run()
}
