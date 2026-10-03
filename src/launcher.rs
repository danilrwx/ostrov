//! The launcher in the bar, dmenu's way: in place of the clock a prompt, what is typed, the apps it matches in a
//! row (or the clipboard's history, clip.rs, a picture marked so, the picked entry shown whole under the bar), the
//! picked one inverted. Left, Right, Tab and Ctrl+N, Ctrl+P move, Enter launches (or copies back), Delete drops a
//! clipboard entry, Escape closes. ostrov run ($mod+d), ostrov clip
//! ($mod+Shift+v).
//!
//! What is typed picks the run's mode, named by the prompt: arithmetic is calculated (calc.rs), its result copied;
//! :name finds emoji, copied; ?question asks Claude (claude -p), the answer shown under the bar and Enter again
//! copying it; g words searches Google in the browser; /name finds files under
//! the home with fd, opened in their default app.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

use gtk4::gdk::Key;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use gtk4::{gio, glib, Orientation};

use crate::hub::run;

/// A hit: what the row shows, and what picking it does.
#[derive(Clone)]
enum Hit {
    App(gio::AppInfo),
    /// an entry of the clipboard's history: its id, whether a picture, what is shown of it
    Clip(u64, bool, String),
    /// a result or an emoji: what is shown, what goes on the clipboard
    Copy(String, String),
    /// a search or a file: what is shown, the URI opened in its default app
    Open(String, String),
    /// a question for Claude (after ?): the question
    Ask(String),
}

impl Hit {
    fn name(&self) -> String {
        match self {
            Hit::App(a) => a.name().to_string(),
            Hit::Clip(_, _, shown) => shown.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect(),
            Hit::Copy(shown, _) | Hit::Open(shown, _) => shown.clone(),
            Hit::Ask(q) => format!("Ask Claude: {q}"),
        }
    }
}

pub struct Launcher {
    pub widget: gtk4::Box,
    prompt: gtk4::Label,
    query: gtk4::Text,
    clip: Cell<bool>,
    all: RefCell<Vec<Hit>>,
    hits: RefCell<Vec<Hit>>,
    /// bumped by every key typed: a file search's late answer to an older query is dropped
    typed: Cell<u64>,
    /// Claude's answer to the question asked, "" while it thinks; None with none asked
    reply: RefCell<Option<String>>,
    picked: Cell<usize>,
    row: gtk4::Box,
    scroll: gtk4::ScrolledWindow,
    on_toggle: Box<dyn Fn(bool)>,
    /// the picked clipboard entry whole, under the bar: its text, or its picture
    pub preview: gtk4::Box,
    preview_text: gtk4::Label,
    preview_picture: gtk4::Picture,
}

impl Launcher {
    /// on_toggle(open): the bar makes room for it and gives it the keyboard.
    pub fn new(on_toggle: impl Fn(bool) + 'static) -> Rc<Launcher> {
        let widget = gtk4::Box::new(Orientation::Horizontal, 10);
        widget.set_hexpand(true);
        widget.set_visible(false);
        widget.set_margin_start(12);
        let prompt = gtk4::Label::new(None);
        prompt.add_css_class("dim");
        let query = gtk4::Text::new();
        query.add_css_class("query");
        query.set_width_chars(24);
        query.set_max_width_chars(24);
        let row = gtk4::Box::new(Orientation::Horizontal, 0);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::External, gtk4::PolicyType::Never);
        scroll.set_hexpand(true);
        scroll.set_child(Some(&row));
        widget.append(&prompt);
        widget.append(&query);
        widget.append(&scroll);

        let preview = gtk4::Box::new(Orientation::Vertical, 0);
        preview.add_css_class("surface");
        preview.add_css_class("preview");
        preview.set_halign(gtk4::Align::Start);
        preview.set_valign(gtk4::Align::Start);
        preview.set_margin_top(crate::popup::BAR + 6);
        preview.set_visible(false);
        preview.set_can_target(false);
        let preview_text = gtk4::Label::new(None);
        preview_text.set_xalign(0.0);
        preview_text.set_wrap(true);
        preview_text.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        preview_text.set_max_width_chars(90);
        preview_text.set_lines(24);
        preview_text.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        preview_text.set_selectable(false);
        let preview_picture = gtk4::Picture::new();
        preview_picture.set_content_fit(gtk4::ContentFit::ScaleDown);
        preview_picture.set_size_request(-1, -1);
        preview_picture.set_can_shrink(true);
        preview.append(&preview_text);
        preview.append(&preview_picture);

        let l = Rc::new(Launcher {
            widget,
            prompt,
            query,
            clip: Cell::new(false),
            all: RefCell::default(),
            hits: RefCell::default(),
            typed: Cell::new(0),
            reply: RefCell::default(),
            picked: Cell::new(0),
            row,
            scroll,
            on_toggle: Box::new(on_toggle),
            preview,
            preview_text,
            preview_picture,
        });
        let l2 = l.clone();
        l.query.connect_changed(move |_| {
            l2.picked.set(0);
            l2.filter();
        });
        let keys = gtk4::EventControllerKey::new();
        let l2 = l.clone();
        keys.connect_key_pressed(move |_, k, _, m| {
            let n = l2.hits.borrow().len();
            let back = m.contains(gtk4::gdk::ModifierType::SHIFT_MASK);
            let ctrl = m.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            match k {
                Key::n if ctrl && n > 0 => l2.set_picked((l2.picked.get() + 1) % n),
                Key::p if ctrl && n > 0 => l2.set_picked((l2.picked.get() + n - 1) % n),
                Key::Escape => l2.close(),
                Key::Return | Key::KP_Enter => l2.pick(l2.picked.get()),
                Key::Delete if l2.clip.get() => l2.delete(),
                Key::Right | Key::Tab if n > 0 && !back => l2.set_picked((l2.picked.get() + 1) % n),
                Key::Left | Key::ISO_Left_Tab | Key::Tab if n > 0 => l2.set_picked((l2.picked.get() + n - 1) % n),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        l.query.add_controller(keys);
        l
    }

    pub fn is_open(&self) -> bool {
        self.widget.is_visible()
    }

    /// Open over the apps, or the clipboard's history; the same again closes it.
    pub fn toggle(self: &Rc<Self>, clip: bool) {
        if self.is_open() && self.clip.get() == clip {
            return self.close();
        }
        self.clip.set(clip);
        *self.all.borrow_mut() = if clip {
            crate::clip::list().into_iter().map(|(id, picture, shown)| Hit::Clip(id, picture, shown)).collect()
        } else {
            gio::AppInfo::all().into_iter().filter(|a| a.should_show()).map(Hit::App).collect()
        };
        self.query.set_text("");
        self.picked.set(0);
        self.filter();
        self.widget.set_visible(true);
        (self.on_toggle)(true);
        self.query.grab_focus();
    }

    /// Opened with a question to Claude typed and asked (ostrov ask QUESTION).
    pub fn ask_now(self: &Rc<Self>, question: &str) {
        if !self.is_open() || self.clip.get() {
            self.toggle(false);
        }
        self.query.set_text(&format!("? {question}"));
        self.query.set_position(-1);
        self.pick(0);
    }

    pub fn close(&self) {
        *self.reply.borrow_mut() = None;
        self.widget.set_visible(false);
        self.preview.set_visible(false);
        (self.on_toggle)(false);
    }

    /// The hits for what is typed, and the mode it puts the run in.
    fn filter(self: &Rc<Self>) {
        let q = self.query.text().to_string();
        self.typed.set(self.typed.get() + 1);
        let (mode, hits) = if self.clip.get() {
            ("clip", self.matching(&q))
        } else if let Some(name) = q.strip_prefix(':') {
            ("emoji", emoji(name))
        } else if let Some((engine, url)) = web(&q) {
            let words = q[2..].trim();
            let uri = format!("{url}{}", glib::Uri::escape_string(words, None, false));
            let hit = Hit::Open(format!("Search {engine} for {words}"), uri);
            ("web", if words.is_empty() { vec![] } else { vec![hit] })
        } else if let Some(question) = q.strip_prefix('?') {
            // a question typed anew: the last answer is gone with it
            *self.reply.borrow_mut() = None;
            let question = question.trim();
            ("claude", if question.is_empty() { vec![] } else { vec![Hit::Ask(question.into())] })
        } else if let Some(name) = q.strip_prefix('/') {
            self.files(name.trim());
            ("files", vec![])
        } else if let Some(r) = crate::calc::eval(&q) {
            ("calc", [vec![Hit::Copy(format!("= {r}"), r)], self.matching(&q)].concat())
        } else {
            ("run", self.matching(&q))
        };
        self.prompt.set_text(mode);
        *self.hits.borrow_mut() = hits;
        self.draw();
    }

    /// Apps by name, a name starting with what is typed first; clips newest first.
    fn matching(&self, q: &str) -> Vec<Hit> {
        let q = q.to_lowercase();
        let all = self.all.borrow();
        let mut hits: Vec<&Hit> = all.iter().filter(|h| h.name().to_lowercase().contains(&q)).collect();
        if !self.clip.get() {
            hits.sort_by_cached_key(|h| {
                let n = h.name().to_lowercase();
                (!n.starts_with(&q), n)
            });
        }
        hits.into_iter().cloned().collect()
    }

    /// Files named so under the home, found by fd off GTK's thread once typing pauses; they arrive as the hits
    /// unless something else has been typed by then.
    fn files(self: &Rc<Self>, name: &str) {
        if name.is_empty() {
            return;
        }
        let (me, name, typed) = (Rc::downgrade(self), name.to_string(), self.typed.get());
        glib::spawn_future_local(async move {
            glib::timeout_future(Duration::from_millis(150)).await;
            if me.upgrade().is_none_or(|me| me.typed.get() != typed) {
                return;
            }
            let (tx, rx) = async_channel::bounded(1);
            std::thread::spawn(move || {
                let _ = tx.send_blocking(fd(&name));
            });
            let Ok(paths) = rx.recv().await else { return };
            let Some(me) = me.upgrade().filter(|me| me.typed.get() == typed && me.is_open()) else { return };
            let home = std::env::var("HOME").unwrap_or_default();
            *me.hits.borrow_mut() = paths
                .into_iter()
                .map(|p| {
                    let shown = p.strip_prefix(&home).map_or(p.clone(), |rest| format!("~{rest}"));
                    Hit::Open(shown, gio::File::for_path(&p).uri().into())
                })
                .collect();
            me.picked.set(0);
            me.draw();
        });
    }

    fn draw(self: &Rc<Self>) {
        crate::style::clear(&self.row);
        // the first hundred: past that no one tabs
        for (n, hit) in self.hits.borrow().iter().take(100).enumerate() {
            let l = gtk4::Box::new(Orientation::Horizontal, 6);
            if let Hit::Clip(_, true, _) = hit {
                l.append(&gtk4::Image::from_icon_name("image-x-generic-symbolic"));
            }
            l.append(&gtk4::Label::new(Some(&hit.name())));
            l.add_css_class("hit");
            if n == self.picked.get() {
                l.add_css_class("picked");
            }
            l.set_cursor_from_name(Some("pointer"));
            let click = gtk4::GestureClick::new();
            let me = Rc::downgrade(self);
            click.connect_released(move |_, _, _, _| {
                if let Some(me) = me.upgrade() {
                    me.pick(n)
                }
            });
            l.add_controller(click);
            self.row.append(&l);
        }
        self.show_preview();
    }

    /// The picked clipboard entry whole under the bar, or Claude's answer; nothing over the apps.
    fn show_preview(&self) {
        if let Some(reply) = self.reply.borrow().as_ref() {
            self.preview_text.set_text(if reply.is_empty() { "…" } else { reply });
            self.preview_text.set_visible(true);
            self.preview_picture.set_visible(false);
            self.preview.set_visible(true);
            return;
        }
        let id = self.hits.borrow().get(self.picked.get()).and_then(|h| match h {
            Hit::Clip(id, ..) => Some(*id),
            _ => None,
        });
        let Some((picture, data)) = id.and_then(crate::clip::content) else {
            self.preview.set_visible(false);
            return;
        };
        if picture {
            // read at most 640 by 400: a full screen's picture would take its own size in the layout
            let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(data));
            let pix = gtk4::gdk_pixbuf::Pixbuf::from_stream_at_scale(&stream, 640, 400, true, gio::Cancellable::NONE).ok();
            #[allow(deprecated)]
            let tex = pix.map(|p| gtk4::gdk::Texture::for_pixbuf(&p));
            self.preview_picture.set_paintable(tex.as_ref());
        } else {
            let text: String = String::from_utf8_lossy(&data).chars().take(4000).collect();
            self.preview_text.set_text(&text);
        }
        self.preview_text.set_visible(!picture);
        self.preview_picture.set_visible(picture);
        self.preview.set_visible(true);
    }

    /// The picked one inverted and scrolled into the row's view.
    fn set_picked(&self, n: usize) {
        if let Some(old) = self.row.observe_children().item(self.picked.get() as u32) {
            old.downcast::<gtk4::Widget>().unwrap().remove_css_class("picked");
        }
        self.picked.set(n);
        self.show_preview();
        let Some(w) = self.row.observe_children().item(n as u32).and_then(|o| o.downcast::<gtk4::Widget>().ok()) else { return };
        w.add_css_class("picked");
        let Some(b) = w.compute_bounds(&self.row) else { return };
        let adj = self.scroll.hadjustment();
        let (x, end) = (b.x() as f64, (b.x() + b.width()) as f64);
        if x < adj.value() {
            adj.set_value(x);
        } else if end > adj.value() + adj.page_size() {
            adj.set_value(end - adj.page_size());
        }
    }

    /// An app launched (a terminal one in alacritty), a clip copied back, a result or an emoji copied, a search or
    /// a file opened; with none, what was typed run, when it is a run.
    fn pick(self: &Rc<Self>, n: usize) {
        let typed = self.query.text().to_string();
        // a question: asked, the launcher kept open for its answer; with the answer there, it copied
        if let Some(Hit::Ask(q)) = self.hits.borrow().get(n).cloned() {
            match self.reply.borrow().clone() {
                Some(answer) if !answer.is_empty() => {
                    self.close();
                    crate::clip::put(answer.into_bytes(), "text");
                }
                Some(_) => {}
                None => self.ask(q),
            }
            return;
        }
        let run_mode = self.prompt.text() == "run";
        let hit = self.hits.borrow().get(n).cloned();
        self.close();
        let ctx = gtk4::gdk::Display::default().map(|d| d.app_launch_context());
        match &hit {
            Some(Hit::App(a)) => {
                let term = a.downcast_ref::<gio_unix::DesktopAppInfo>().is_some_and(|d| d.boolean("Terminal"));
                if term {
                    let cmd = a.commandline().map(|c| c.to_string_lossy().into_owned()).unwrap_or_default();
                    let cmd: Vec<&str> = cmd.split_whitespace().filter(|w| !w.starts_with('%')).collect();
                    run(&[&["alacritty", "-e"], &cmd[..]].concat());
                } else {
                    let _ = a.launch(&[], ctx.as_ref());
                }
            }
            Some(Hit::Clip(id, ..)) => crate::clip::copy(*id),
            Some(Hit::Copy(_, text)) => crate::clip::put(text.clone().into_bytes(), "text"),
            Some(Hit::Open(_, uri)) => {
                if let Err(e) = gio::AppInfo::launch_default_for_uri(uri, ctx.as_ref()) {
                    eprintln!("ostrov: open {uri}: {e}");
                }
            }
            // asked above, before the launcher closed
            Some(Hit::Ask(_)) => {}
            None if run_mode && !typed.trim().is_empty() => run(&["sh", "-c", &typed]),
            None => {}
        }
    }

    /// The question put to Claude Code (claude -p) off GTK's thread, "…" under the bar till its answer comes;
    /// an answer to a question since typed over is dropped.
    fn ask(self: &Rc<Self>, question: String) {
        *self.reply.borrow_mut() = Some(String::new());
        self.show_preview();
        let (me, typed) = (Rc::downgrade(self), self.typed.get());
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let claude = crate::hub::home().join(".local/bin/claude");
            let out = std::process::Command::new(claude)
                .args(["-p", "--output-format", "text", "--append-system-prompt"])
                .arg("Answer briefly and in plain text, no Markdown: the answer is read in a small popup under the \
                      desktop's bar, and copied from there.")
                .arg(&question)
                .current_dir(crate::hub::home())
                .stdin(std::process::Stdio::null())
                .output();
            let answer = match out {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
                Ok(o) => format!("claude: {}", String::from_utf8_lossy(&o.stderr).trim()),
                Err(e) => format!("claude: {e}"),
            };
            let _ = tx.send_blocking(answer);
        });
        glib::spawn_future_local(async move {
            let Ok(answer) = rx.recv().await else { return };
            let Some(me) = me.upgrade().filter(|me| me.typed.get() == typed && me.is_open()) else { return };
            *me.reply.borrow_mut() = Some(if answer.is_empty() { "(no answer)".into() } else { answer });
            me.show_preview();
        });
    }

    /// The picked clip out of the history.
    fn delete(self: &Rc<Self>) {
        let Some(Hit::Clip(id, ..)) = self.hits.borrow().get(self.picked.get()).cloned() else { return };
        crate::clip::delete(id);
        self.all.borrow_mut().retain(|h| !matches!(h, Hit::Clip(i, ..) if *i == id));
        let n = self.picked.get();
        self.filter();
        self.set_picked(n.min(self.hits.borrow().len().saturating_sub(1)));
    }
}

/// "g words" searches Google: the engine's name and its query's URL.
fn web(q: &str) -> Option<(&'static str, &'static str)> {
    match q.get(..2)? {
        "g " => Some(("Google", "https://www.google.com/search?q=")),
        _ => None,
    }
}

/// Emoji whose name has what is typed, a name starting with it first.
fn emoji(q: &str) -> Vec<Hit> {
    static ALL: OnceLock<Vec<(String, String)>> = OnceLock::new();
    let all = ALL.get_or_init(|| {
        parse_emoji(&std::fs::read_to_string("/usr/share/unicode/emoji/emoji-test.txt").unwrap_or_default())
    });
    let q = q.trim().to_lowercase();
    let mut hits: Vec<_> = all.iter().filter(|(_, name)| name.contains(&q)).collect();
    hits.sort_by_key(|(_, name)| !name.starts_with(&q));
    hits.into_iter().map(|(e, name)| Hit::Copy(format!("{e} {name}"), e.clone())).collect()
}

/// emoji-test.txt's fully-qualified emoji and their names:
/// `1F600 ; fully-qualified # 😀 E1.0 grinning face`, the name past the version it came in.
fn parse_emoji(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter(|l| l.contains("; fully-qualified"))
        .filter_map(|l| {
            let (_, rest) = l.split_once("# ")?;
            let (e, rest) = rest.split_once(' ')?;
            let (_, name) = rest.split_once(' ')?;
            Some((e.to_string(), name.to_string()))
        })
        .collect()
}

/// Up to fifty paths under the home whose name has name in it, by fd (fdfind on Debian's); none without it.
fn fd(name: &str) -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let args = ["--fixed-strings", "--absolute-path", "--max-results", "50", "--", name, &home];
    let Some(out) = ["fd", "fdfind"].iter().find_map(|fd| std::process::Command::new(fd).args(args).output().ok())
    else {
        return vec![];
    };
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_emoji() {
        let text = "# group: Smileys\n1F600 ; fully-qualified # 😀 E1.0 grinning face\n\
                    263A ; unqualified # ☺ E0.6 smiling face\n";
        assert_eq!(super::parse_emoji(text), [("😀".to_string(), "grinning face".to_string())]);
    }

    #[test]
    fn web() {
        assert_eq!(super::web("g rust gtk").map(|w| w.0), Some("Google"));
        assert_eq!(super::web("gimp"), None);
    }
}
