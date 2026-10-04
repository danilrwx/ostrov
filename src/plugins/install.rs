//! `ostrov plugin install PATH|GIT-URL` and `ostrov plugin remove ID`. A plugin installed is copied into
//! ~/.local/share/ostrov/plugins/<id>/ (its id its manifest's), its symlinks resolved (an example's link to the
//! SDK becomes the SDK); a git URL cloned first, its history left behind. Its manifest checked, its permissions
//! said in a dialog before it is put there, then started at once: its commands, keys, launcher modes, events and
//! calendar work without a restart, its widgets join the gallery at the next (the registry is made once). A
//! plugin installed again over a running one is replaced on disk, and runs as new after a restart. Removed, its
//! process ends, its keys and modes and calendar go at once, its widgets at the next restart.

use std::path::{Path, PathBuf};

use gtk4::gio;
use serde_json::json;

use super::{bind_keys, dir, load, parse_manifest, word, Manifest, MODES, PLUGINS};

/// Whether an install's source is a git URL rather than a path: a URL's scheme, or ssh's git@host:path.
pub fn is_url(src: &str) -> bool {
    src.contains("://") || src.starts_with("git@")
}

/// What a permission lets a plugin do, for the dialog asking whether to install it.
fn grants(m: &Manifest, p: &str) -> String {
    match p {
        "run" => "run any of ostrov's commands".into(),
        "network" => "fetch from the network".into(),
        "secrets" => "read its secrets from the keyring".into(),
        "dialogs" => "ask you things in dialogs".into(),
        "state" => "see the desktop's state (Wi-Fi, Bluetooth, media...)".into(),
        "events" => format!("follow what happens on the desktop ({})", m.events.join(", ")),
        "keys" => {
            let keys: Vec<String> = m.keys.iter().map(|k| format!("{}: {}", k.combo, k.command)).collect();
            format!("bind keys where free ({})", keys.join("; "))
        }
        "calendar" => "add its events to the calendar".into(),
        other => other.to_string(),
    }
}

/// The dialog's words: what it is, what its manifest asks for, and that a process can do more.
fn about(m: &Manifest) -> String {
    let mut s = format!("{} {}", m.id, m.version).trim().to_string();
    if !m.description.is_empty() {
        s += &format!(": {}", m.description);
    }
    if m.permissions.is_empty() {
        s += "\n\nIt asks for no permissions.";
    } else {
        s += "\n\nIt may:";
        for p in &m.permissions {
            s += &format!("\n• {}", grants(m, p));
        }
    }
    s + "\n\nA plugin is a program run as you: it can do whatever you can, whatever it asks for."
}

/// A directory copied into another, its symlinks followed (not too deep: a link to a parent would go round), a
/// .git left out. Blocking.
fn copy(from: &Path, to: &Path, depth: u32) -> Result<(), String> {
    if depth > 16 {
        return Err(format!("{}: too deep (a symlink going round?)", from.display()));
    }
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for e in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        if e.file_name() == ".git" {
            continue;
        }
        let (src, dst) = (e.path(), to.join(e.file_name()));
        let meta = std::fs::metadata(&src).map_err(|err| format!("{}: {err}", src.display()))?;
        if meta.is_dir() {
            copy(&src, &dst, depth + 1)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|err| format!("{}: {err}", src.display()))?;
        }
    }
    Ok(())
}

/// A git URL cloned, its last commit alone, into a directory beside the plugins'. Blocking.
fn clone(url: &str, into: &Path) -> Result<(), String> {
    let out = std::process::Command::new("git")
        .args(["clone", "--depth", "1", "--quiet", "--", url])
        .arg(into)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("git clone {url}: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// A blocking job on GLib's pool, awaited on GTK's thread.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    gio::spawn_blocking(f).await.map_err(|_| "the install's job panicked".to_string())?
}

/// The manifest in a plugin's directory, read and checked.
fn manifest(at: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(at.join("manifest.toml"))
        .map_err(|e| format!("{}: {e}", at.join("manifest.toml").display()))?;
    let id: Manifest = toml::from_str(&text).map_err(|e| format!("manifest.toml: {e}"))?;
    parse_manifest(&text, &id.id).map_err(|e| format!("manifest.toml: {e}"))
}

/// `ostrov plugin install SOURCE`: a git URL cloned, then installed as a path is.
pub async fn install(src: String) -> Result<String, String> {
    if !is_url(&src) {
        return from(PathBuf::from(src)).await;
    }
    let tmp = dir().with_file_name(format!(".clone-{}", std::process::id()));
    let (url, at) = (src.clone(), tmp.clone());
    let r = match blocking(move || {
        let _ = std::fs::remove_dir_all(&at);
        clone(&url, &at)
    })
    .await
    {
        Ok(()) => from(tmp.clone()).await,
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

/// A plugin's directory checked, asked about, copied in and started.
async fn from(src: PathBuf) -> Result<String, String> {
    let m = manifest(&src)?;
    let dest = dir().join(&m.id);
    if src.canonicalize().ok() == dest.canonicalize().ok() {
        return Err(format!("{} is where plugin {} lives already", src.display(), m.id));
    }
    let running = PLUGINS.with(|ps| ps.borrow().iter().any(|p| p.m.id == m.id));
    let spec = json!({
        "kind": "confirm",
        "icon": if m.icon.is_empty() { "application-x-addon-symbolic" } else { m.icon.as_str() },
        "title": format!("{} {}?", if running { "Replace the plugin" } else { "Install the plugin" }, m.name),
        "text": about(&m),
        "ok": if running { "Replace" } else { "Install" },
    });
    crate::prompt::dialog(&spec, None, None).await?.ok_or("not installed")?;
    // copied beside its place and moved in, so a copy that fails leaves the old one whole
    let new = dir().join(format!(".{}.new", m.id));
    let (s, d, n) = (src.clone(), dest.clone(), new.clone());
    blocking(move || {
        let _ = std::fs::remove_dir_all(&n);
        copy(&s, &n, 0)?;
        if d.exists() {
            std::fs::remove_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::rename(&n, &d).map_err(|e| format!("{}: {e}", d.display()))
    })
    .await?;
    if running {
        return Ok(format!("plugin {} replaced in {}: `ostrov restart` to run it", m.id, dest.display()));
    }
    let (id, widgets) = (m.id.clone(), !m.widgets.is_empty());
    let p = load(dest.clone(), m, &crate::config::load());
    PLUGINS.with(|ps| {
        let mut ps = ps.borrow_mut();
        ps.push(p);
        ps.sort_by(|a, b| a.m.id.cmp(&b.m.id));
    });
    bind_keys(None);
    let later = if widgets { "; its widgets join the gallery after `ostrov restart`" } else { "" };
    Ok(format!("plugin {id} installed in {} and started{later}", dest.display()))
}

/// `ostrov plugin remove ID`: its process ended, its modes, keys and calendar gone, its directory deleted.
pub fn remove(id: &str) -> Result<String, String> {
    if !word(id) {
        return Err(format!("no plugin {id:?}"));
    }
    let at = dir().join(id);
    let p = PLUGINS.with(|ps| {
        let mut ps = ps.borrow_mut();
        let i = ps.iter().position(|p| p.m.id == id)?;
        Some(ps.remove(i))
    });
    if p.is_none() && !at.exists() {
        return Err(format!("no plugin {id}"));
    }
    let mut later = "";
    if let Some(p) = &p {
        if let Some(t) = &p.task {
            t.abort();
        }
        MODES.with(|ms| ms.borrow_mut().retain(|m| m.id != id));
        crate::modules::calendar::service::source(id, None);
        bind_keys(Some(&p.m));
        if !p.m.widgets.is_empty() || p.settings.borrow().is_some() {
            later = "; its widgets and settings leave after `ostrov restart`";
        }
    }
    if at.exists() {
        std::fs::remove_dir_all(&at).map_err(|e| format!("{}: {e}", at.display()))?;
    }
    Ok(format!("plugin {id} removed{later}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources() {
        assert!(is_url("https://example.org/p.git"));
        assert!(is_url("git@example.org:me/p.git"));
        assert!(!is_url("examples/plugins/hello-python"));
        assert!(!is_url("/tmp/p"));
    }

    #[test]
    fn copied_with_links_resolved() {
        let root = std::env::temp_dir().join(format!("ostrov-install-{}", std::process::id()));
        let (src, dst) = (root.join("src"), root.join("dst"));
        std::fs::create_dir_all(src.join("lib")).unwrap();
        std::fs::create_dir_all(src.join(".git")).unwrap();
        std::fs::write(root.join("sdk.py"), "sdk").unwrap();
        std::fs::write(src.join("lib/a"), "a").unwrap();
        std::os::unix::fs::symlink(root.join("sdk.py"), src.join("sdk.py")).unwrap();
        copy(&src, &dst, 0).unwrap();
        assert_eq!(std::fs::read_to_string(dst.join("lib/a")).unwrap(), "a");
        assert!(!std::fs::symlink_metadata(dst.join("sdk.py")).unwrap().is_symlink());
        assert_eq!(std::fs::read_to_string(dst.join("sdk.py")).unwrap(), "sdk");
        assert!(!dst.join(".git").exists());
        // a link to its own parent goes round, and is said
        std::os::unix::fs::symlink(&src, src.join("lib/up")).unwrap();
        assert!(copy(&src, &root.join("dst2"), 0).unwrap_err().contains("too deep"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
