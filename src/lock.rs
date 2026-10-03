//! The lock screen, in the bar's look: black, the time large in the middle, the date under it, the password in a
//! white-ruled box, a wrong one said in red. Through Wayland's session lock (the compositor shows nothing else
//! until it lets go, gtk4-session-lock) and ostrov's PAM profile (login's auth). ostrov lock: $mod+Shift+x,
//! the quick settings' button (loginctl lock-session), swayidle on idle and before sleep (bin/wl-autostart).

use std::ffi::{c_char, c_int, c_void, CString};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Align, Orientation};
use gtk4_session_lock::Instance;

pub struct Lock {
    inst: Instance,
}

impl Lock {
    pub fn lock(&self) {
        if !self.inst.is_locked() {
            self.inst.lock();
        }
    }
}

pub fn build(app: &gtk4::Application) -> Rc<Lock> {
    let inst = Instance::new();
    let app = app.clone();
    inst.connect_monitor(move |inst, monitor| {
        let win = gtk4::ApplicationWindow::new(&app);
        win.add_css_class("lock");
        win.set_child(Some(&face(inst)));
        inst.assign_window_to_monitor(&win, monitor);
        win.present();
    });
    Rc::new(Lock { inst })
}

/// A monitor's lock surface: the time, the date, the password.
fn face(inst: &Instance) -> gtk4::Box {
    let col = gtk4::Box::new(Orientation::Vertical, 14);
    col.set_halign(Align::Center);
    col.set_valign(Align::Center);
    clock(&col);
    let entry = gtk4::PasswordEntry::new();
    entry.add_css_class("lock-entry");
    entry.set_alignment(0.5);
    entry.set_placeholder_text(Some("password"));
    let error = gtk4::Label::new(None);
    error.add_css_class("error");
    col.append(&entry);
    col.append(&error);
    entry.connect_map(|e| {
        e.grab_focus();
    });

    let inst = inst.clone();
    entry.connect_activate(move |e| {
        let password = e.text().to_string();
        if password.is_empty() {
            return;
        }
        e.set_text("");
        e.set_sensitive(false);
        error.remove_css_class("error");
        error.add_css_class("dim");
        error.set_text("…");
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(authenticate(&password));
        });
        let (e, error, inst) = (e.clone(), error.clone(), inst.clone());
        glib::spawn_future_local(async move {
            let ok = rx.recv().await.unwrap_or(false);
            e.set_sensitive(true);
            error.remove_css_class("dim");
            error.add_css_class("error");
            if ok {
                error.set_text("");
                inst.unlock();
            } else {
                error.set_text("Wrong password");
                e.grab_focus();
            }
        });
    });
    col
}

/// The time large and the date under it, ticking, appended to a column: the lock screen's and the login screen's.
pub fn clock(col: &gtk4::Box) {
    let time = gtk4::Label::new(None);
    time.add_css_class("lock-time");
    let date = gtk4::Label::new(None);
    date.add_css_class("dim");
    date.set_margin_bottom(30);
    col.append(&time);
    col.append(&date);
    let tick = {
        let (time, date) = (time.downgrade(), date.downgrade());
        move || {
            let (Some(time), Some(date)) = (time.upgrade(), date.upgrade()) else { return glib::ControlFlow::Break };
            if let Ok(now) = glib::DateTime::now_local() {
                time.set_text(&now.format("%H:%M").unwrap_or_default());
                date.set_text(&now.format("%A, %-d %B").unwrap_or_default());
            }
            glib::ControlFlow::Continue
        }
    };
    tick();
    glib::timeout_add_seconds_local(1, tick);
}

// PAM, by hand: the three calls a check of a password takes and the conversation that hands it over
#[repr(C)]
struct PamMessage {
    style: c_int,
    msg: *const c_char,
}
#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    retcode: c_int,
}
#[repr(C)]
struct PamConv {
    conv: extern "C" fn(c_int, *mut *const PamMessage, *mut *mut PamResponse, *mut c_void) -> c_int,
    data: *mut c_void,
}
#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(service: *const c_char, user: *const c_char, conv: *const PamConv, h: *mut *mut c_void) -> c_int;
    fn pam_authenticate(h: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(h: *mut c_void, status: c_int) -> c_int;
}
unsafe extern "C" {
    fn calloc(n: usize, size: usize) -> *mut c_void;
    fn strdup(s: *const c_char) -> *mut c_char;
}

/// The password to every prompt PAM asks with (echo off or on); PAM frees the answers.
extern "C" fn converse(n: c_int, msgs: *mut *const PamMessage, out: *mut *mut PamResponse, data: *mut c_void) -> c_int {
    unsafe {
        let resp = calloc(n as usize, size_of::<PamResponse>()) as *mut PamResponse;
        if resp.is_null() {
            return 5; // PAM_BUF_ERR
        }
        for i in 0..n as usize {
            let m = &**msgs.add(i);
            if m.style == 1 || m.style == 2 {
                (*resp.add(i)).resp = strdup(data as *const c_char);
            }
        }
        *out = resp;
    }
    0
}

/// The user's password right, by ostrov's PAM profile (/etc/pam.d/ostrov, the login's auth: dotfiles' install
/// puts it), by the login's itself where it is not there yet: a profile missing would leave PAM's "other", which
/// takes no password, and the screen locked for good.
fn authenticate(password: &str) -> bool {
    let (Ok(user), Ok(pw)) = (CString::new(std::env::var("USER").unwrap_or_default()), CString::new(password)) else {
        return false;
    };
    let conv = PamConv { conv: converse, data: pw.as_ptr() as *mut c_void };
    let mut h = std::ptr::null_mut();
    unsafe {
        let service = if std::path::Path::new("/etc/pam.d/ostrov").exists() { c"ostrov" } else { c"login" };
        if pam_start(service.as_ptr(), user.as_ptr(), &conv, &mut h) != 0 {
            return false;
        }
        let r = pam_authenticate(h, 0);
        pam_end(h, r);
        r == 0
    }
}
