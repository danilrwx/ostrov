//! ostrov ("island" in Russian): the desktop's shell, everything in one place, in Rust on GTK4 and
//! gtk4-layer-shell. The bar, made of blocks (bar/), with the popups grown out of them in its own window
//! (popup.rs): the quick settings, the calendar, the tray's menus; the launcher in the bar, notifications, the
//! lock screen; the desktop's state and switches from its services (services/, modules/), the compositor's
//! through wm.rs, how it all looks in style.rs. One ostrov runs: `ostrov ARGS` hands ARGS to it.

mod api;
mod backend;
mod bar;
mod bars;
mod calc;
mod cc;
mod clip;
mod config;
mod doctor;
mod events;
mod forms;
mod hub;
mod i18n;
mod idle;
mod keys;
mod launcher;
mod lock;
mod modules;
mod look;
mod notes;
mod plugins;
mod polkit;
mod popup;
mod prompt;
mod services;
mod settings;
mod share;
mod shot;
mod style;
mod theme;
mod ui;
mod wallpaper;
mod welcome;
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
    /// The screenshot's selector, which `pick-region` asks a region with, set once it is built.
    static SHOT: std::cell::RefCell<Option<Rc<shot::Shot>>> = Default::default();
}

/// ostrov's own commands, in forms.rs's grammar; besides them a bar block's (BLOCK ARGS) and a module's (MODULE
/// ARGS).
const FORMS: &[&str] = &[
    "panel", "menu NAME", "settings [SECTION]", "appearance", "calendar", "run", "clip",
    "lock", "restart", "hyprland", "key NAME", "awake", "screenshot", "capture FILE", "pick-region",
    "share-pick [--allow-token]",
    "bar toggle|peek|unpeek", "state", "dump", "toast TITLE [BODY...]", "dialog JSON", "plugins",
    "plugin [ID] [ARGS...]", "plugin install SOURCE", "plugin remove ID",
    "theme list", "theme set ID", "theme install PATH|GIT-URL", "theme remove ID",
    "help", "complete [WORD...]", "completions zsh|bash|fish", "doctor", "welcome",
];

fn usage() -> String {
    forms::usage("", FORMS) + " | BLOCK ARGS | MODULE ARGS"
}

fn activate(app: &gtk4::Application) {
    style::load();
    // the polkit agent before anything else: polkit takes the first one registered in the session, and an agent
    // autostarted alongside (lxpolkit's XDG autostart) would have the passwords asked in its look
    let prompts = prompt::Prompts::new(app);
    polkit::start(&prompts);
    wallpaper::start(app);
    clip::start();

    let hub = Hub::start();
    let notes = notes::start(app);
    let cfg = config::load();
    // the plugins' widgets before the control centre takes them; ostrov's D-Bus face (api.rs)
    events::watch(&hub);
    plugins::start(&hub);
    api::start(&hub);
    // a bar on every monitor (bars.rs), each its window with the popups laid over it (popup.rs); the launcher over
    // the first's strip
    let bars = bars::Bars::start(app, &hub, cfg.bar);
    let (host, bar, over) = bars.first();

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
    SHOT.with(|s| *s.borrow_mut() = Some(shot.clone()));
    keys::battery(&hub, &notes, &prompts);
    let keys = keys::Keys::new(&hub, &notes);
    PROMPTS.with(|p| *p.borrow_mut() = Some(prompts));
    let welcome = welcome::Welcome::new(app);
    welcome.first_run();

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
                ["settings", ref entry @ ..] => panel("control")?.toggle_page("settings", entry.first().copied()),
                ["appearance"] => panel("control")?.toggle_page("appearance", None),
                ["run"] => launcher.toggle(false),
                ["clip"] => launcher.toggle(true),
                ["lock"] => lock.lock(),
                ["restart"] => restart(),
                ["welcome"] => welcome.open(),
                ["hyprland"] => return Ok(modules::hyprland::conf()),
                ["screenshot"] => shot.take(),
                ["awake"] => idle::set_awake(!idle::awake()),
                ["capture", path] => shot::capture(path.to_string()),
                ["key", name] => keys.key(name)?,
                ["bar", what @ ("toggle" | "peek" | "unpeek")] => {
                    let docked = !host.docked.get();
                    for (h, ..) in bars.all.borrow().iter() {
                        match what {
                            "toggle" => h.docked.set(docked),
                            "peek" => h.peeking.set(true),
                            _ => h.peeking.set(false),
                        }
                        h.apply();
                    }
                }
                // the services' state, as JSON
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
                        themes: theme::all().into_iter().map(|t| (t.id, t.name)).collect(),
                    };
                    return Ok(forms::lines(&forms::complete(&known, if words.is_empty() { &[""] } else { words })));
                }
                ["help"] => {
                    return Ok(format!("{}\n\nthe modules' commands:\n{}\n\nthe plugins' commands:\n{}", usage(), services::usage(), plugins::help()));
                }
                // what is open, and the bar's mode: for a script, a test
                ["state"] => {
                    let all = bars.all.borrow();
                    let mut words: Vec<&str> = all.iter().find_map(|(_, b, _)| b.open()).into_iter().collect();
                    words.push(if host.docked.get() { "docked" } else { "hidden" });
                    if launcher.is_open() {
                        words.push("launcher");
                    }
                    if idle::awake() {
                        words.push("awake");
                    }
                    return Ok(words.join(" "));
                }
                // the focused monitor's bar's
                [block, ref rest @ ..] if bar.has_command(block) => {
                    return bars.focused().command(block, rest).unwrap_or(Ok(String::new()));
                }
                // the modules' commands: their outcome said by the running ostrov
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
        // the region waits for the user: answered once dragged out (plugins/record's, in place of slurp)
        [first] if first == "pick-region" => {
            let shot = SHOT.with(|s| s.borrow().clone());
            return Box::pin(async move { shot.ok_or("ostrov is starting")?.pick().await });
        }
        // xdph's screen-share picker (share.rs): the line it reads once picked, the region dragged out after
        [first, rest @ ..] if first == "share-pick" => {
            let shot = SHOT.with(|s| s.borrow().clone());
            return Box::pin(share::pick(rest.to_vec(), shot));
        }
        // a theme's install clones a repository: awaited, ostrov going on meanwhile
        [first, rest @ ..] if first == "theme" => return Box::pin(theme::command(rest.to_vec())),
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

unsafe extern "C" {
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
}

/// The children an ostrov before this one left: restart execs in place, so its plugins and commands are this one's
/// now, unknown to what waits for children here, each a zombie as it ends. Found as ostrov starts, before it has
/// any of its own, and waited for on a thread until every one has ended.
fn adopt() {
    let me = std::process::id().to_string();
    let mut left: Vec<i32> = std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
            // pid (comm) state ppid ...: after the comm's last paren, its name being any text
            let ppid = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?;
            (ppid == me).then(|| e.file_name().to_str()?.parse().ok())?
        })
        .collect();
    if left.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        const WNOHANG: i32 = 1;
        while !left.is_empty() {
            // SAFETY: waitpid on pids that are this process's children, a null status not written
            left.retain(|&pid| unsafe { waitpid(pid, std::ptr::null_mut(), WNOHANG) } == 0);
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    });
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

/// ssh's askpass (SSH_ASKPASS, a script running `exec ostrov askpass "$@"`): `ostrov askpass [--confirm|--none]
/// PROMPT`, a key's passphrase asked and printed, or a yes or no to using a key (--confirm: the exit status says it;
/// --none: a word alone). The asking ostrov waits for the answer: the command line kept until it comes, its status set
/// then.
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
    // one ostrov: run again, it hands its arguments to the running one and exits
    // OSTROV_APP_ID: another id, a second ostrov beside the running one (a build tried out without stopping it)
    adopt();
    let id = std::env::var("OSTROV_APP_ID").unwrap_or_else(|_| "dev.ostrov.Ostrov".into());
    i18n::follow_locale();
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
    // xdph's screen-share picker (share.rs), run by it as ostrov-share-picker (a link to ostrov: it takes a path and
    // no arguments) or as `ostrov share-pick`: its windows' list in this environment, not the running ostrov's,
    // handed on as an argument; with no ostrov running, its own Qt picker
    let mut argv: Vec<String> = std::env::args().collect();
    let linked = argv.first().is_some_and(|a| std::path::Path::new(a).ends_with("ostrov-share-picker"));
    if linked || argv.get(1).is_some_and(|a| a == "share-pick") {
        let flags = argv.split_off(if linked { 1 } else { 2 });
        if !forms::running(&id) {
            use std::os::unix::process::CommandExt;
            let e = std::process::Command::new("hyprland-share-picker").args(&flags).exec();
            eprintln!("ostrov: hyprland-share-picker: {e}");
            return glib::ExitCode::FAILURE;
        }
        let list = std::env::var("XDPH_WINDOW_SHARING_LIST").unwrap_or_default();
        argv = [argv[0].clone(), "share-pick".into(), list].into_iter().chain(flags).collect();
    }
    let app = gtk4::Application::builder()
        .application_id(id.as_str())
        .flags(gtk4::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    app.connect_activate(activate);
    app.connect_command_line(|app, cl| {
        let mut args: Vec<String> = cl.arguments().iter().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
        // a plugin's path to install from is the asking shell's, relative to its directory
        if let [p, i, src] = &mut args[..]
            && p == "plugin"
            && i == "install"
            && !plugins::is_url(src)
            && let Some(cwd) = cl.cwd()
        {
            *src = cwd.join(&*src).to_string_lossy().into_owned();
        }
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
    app.run_with_args(&argv)
}
