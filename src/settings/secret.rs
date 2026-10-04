//! Secrets kept out of config.toml: in the Secret Service (GNOME Keyring, KeePassXC...), its default collection,
//! an item a key with the attributes {app: "ostrov", key: "<section>.<key>"} (secret-tool lookup app ostrov key
//! calendar.password finds it). Without a Secret Service, ~/.local/share/ostrov/secrets.toml, the user's alone.
//! Every call blocks (a locked keyring asks for its password first), so none is made on GTK's thread.

use std::collections::HashMap;
use std::future::Future;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use crate::services::dbus::{call, err};

const BUS: &str = "org.freedesktop.secrets";
const SERVICE: &str = "/org/freedesktop/secrets";
/// Long enough for the keyring's password to be typed into its prompt.
const TIMEOUT: Duration = Duration::from_secs(120);

/// A secret as the Secret Service hands it: its session, parameters, value, content type.
type Secret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

fn attrs(key: &str) -> HashMap<&str, &str> {
    HashMap::from([("app", "ostrov"), ("key", key)])
}

fn file() -> PathBuf {
    crate::hub::home().join(".local/share/ostrov/secrets.toml")
}

/// The secret kept for key ("calendar.password"), from the Secret Service, else the file.
pub fn secret(key: &str) -> Option<String> {
    match block(lookup(key.to_string())) {
        Ok(Some(v)) => Some(v),
        _ => file_get(&file(), key),
    }
}

/// Whether a secret is kept for key, without unlocking anything to see it.
pub fn exists(key: &str) -> bool {
    block(search(key.to_string())).is_ok_and(|(open, locked)| !open.is_empty() || !locked.is_empty())
        || file_get(&file(), key).is_some()
}

/// The secret for key set (None: taken out): in the Secret Service if there is one (and then out of the file),
/// else in the file.
pub fn store(key: &str, value: Option<&str>) -> Result<(), String> {
    let (k, v) = (key.to_string(), value.map(String::from));
    let kept = block(async move {
        match opened().await {
            Ok(_) => keep(&k, v.as_deref()).await.map(|_| true),
            Err(_) => Ok(false),
        }
    });
    match kept {
        Ok(true) => file_set(&file(), key, None).or(Ok(())),
        Ok(false) => file_set(&file(), key, value),
        Err(e) => Err(e),
    }
}

/// f run to its end on a Tokio runtime of its own on a thread of its own: callable from GTK's side (off its
/// thread) and from the services' blocking tasks alike.
fn block<T: Send>(f: impl Future<Output = Result<T, String>> + Send) -> Result<T, String> {
    std::thread::scope(|s| {
        s.spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
            rt.block_on(async {
                tokio::time::timeout(TIMEOUT, f).await.map_err(|_| "the Secret Service did not answer".to_string())?
            })
        })
        .join()
        .unwrap_or_else(|_| Err("the Secret Service's thread failed".into()))
    })
}

/// The session bus, a plain session (the bus is the user's own) and the default collection.
async fn opened() -> Result<(zbus::Connection, OwnedObjectPath, OwnedObjectPath), String> {
    let c = zbus::Connection::session().await.map_err(err)?;
    let (_, session): (OwnedValue, OwnedObjectPath) =
        call(&c, BUS, SERVICE, "org.freedesktop.Secret.Service.OpenSession", &("plain", Value::from(""))).await?;
    let coll: OwnedObjectPath =
        call(&c, BUS, SERVICE, "org.freedesktop.Secret.Service.ReadAlias", &("default",)).await?;
    if coll.as_str() == "/" {
        return Err("no default collection".into());
    }
    Ok((c, session, coll))
}

/// The items for key, unlocked and locked.
async fn search(key: String) -> Result<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>), String> {
    let c = zbus::Connection::session().await.map_err(err)?;
    call(&c, BUS, SERVICE, "org.freedesktop.Secret.Service.SearchItems", &(attrs(&key),)).await
}

async fn lookup(key: String) -> Result<Option<String>, String> {
    let (c, session, _) = opened().await?;
    let (mut found, locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) =
        call(&c, BUS, SERVICE, "org.freedesktop.Secret.Service.SearchItems", &(attrs(&key),)).await?;
    if found.is_empty() && !locked.is_empty() {
        unlock(&c, &locked).await?;
        found = locked;
    }
    let Some(item) = found.first() else { return Ok(None) };
    let (_, _, value, _): Secret =
        call(&c, BUS, item.as_str(), "org.freedesktop.Secret.Item.GetSecret", &(&session,)).await?;
    Ok(Some(String::from_utf8_lossy(&value).into_owned()))
}

/// The secret set or taken out in the default collection, unlocked first.
async fn keep(key: &str, value: Option<&str>) -> Result<(), String> {
    let (c, session, coll) = opened().await?;
    unlock(&c, std::slice::from_ref(&coll)).await?;
    let Some(value) = value else {
        let (open, locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) =
            call(&c, BUS, SERVICE, "org.freedesktop.Secret.Service.SearchItems", &(attrs(key),)).await?;
        for item in open.iter().chain(&locked) {
            let p: OwnedObjectPath = call(&c, BUS, item.as_str(), "org.freedesktop.Secret.Item.Delete", &()).await?;
            prompt(&c, &p).await?;
        }
        return Ok(());
    };
    let label = format!("ostrov: {key}");
    let props = HashMap::from([
        ("org.freedesktop.Secret.Item.Label", Value::from(label.as_str())),
        ("org.freedesktop.Secret.Item.Attributes", Value::from(attrs(key))),
    ]);
    let secret = (&session, Vec::<u8>::new(), value.as_bytes().to_vec(), "text/plain");
    let (_, p): (OwnedObjectPath, OwnedObjectPath) =
        call(&c, BUS, coll.as_str(), "org.freedesktop.Secret.Collection.CreateItem", &(props, secret, true)).await?;
    prompt(&c, &p).await
}

async fn unlock(c: &zbus::Connection, objects: &[OwnedObjectPath]) -> Result<(), String> {
    let (_, p): (Vec<OwnedObjectPath>, OwnedObjectPath) =
        call(c, BUS, SERVICE, "org.freedesktop.Secret.Service.Unlock", &(objects,)).await?;
    prompt(c, &p).await
}

/// The keyring's own prompt (its password, a confirmation) shown and waited out; "/" is none needed.
async fn prompt(c: &zbus::Connection, path: &OwnedObjectPath) -> Result<(), String> {
    if path.as_str() == "/" {
        return Ok(());
    }
    let p = zbus::Proxy::new(c, BUS, path.as_str().to_string(), "org.freedesktop.Secret.Prompt").await.map_err(err)?;
    let mut done = p.receive_signal("Completed").await.map_err(err)?;
    p.call::<_, _, ()>("Prompt", &("",)).await.map_err(err)?;
    let m = done.next().await.ok_or("the keyring's prompt went away")?;
    let (dismissed, _): (bool, OwnedValue) = m.body().deserialize().map_err(err)?;
    if dismissed { Err("the keyring's prompt was dismissed".into()) } else { Ok(()) }
}

fn file_get(path: &Path, key: &str) -> Option<String> {
    let t: toml::Table = std::fs::read_to_string(path).ok()?.parse().ok()?;
    t.get(key)?.as_str().map(String::from)
}

/// The file with key set or taken out, written readable by the user alone.
fn file_set(path: &Path, key: &str, value: Option<&str>) -> Result<(), String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
    match value {
        Some(v) => doc[key] = toml_edit::value(v),
        None if doc.contains_key(key) => drop(doc.remove(key)),
        None => return Ok(()),
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| e.to_string())?;
    // a file made before with looser bits
    f.set_permissions(std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    f.write_all(doc.to_string().as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against the session's own Secret Service, read alone: cargo test keyring -- --ignored
    #[test]
    #[ignore]
    fn keyring_read() {
        assert!(block(opened()).is_ok());
        assert_eq!(block(lookup("ostrov.test.none".into())), Ok(None));
        assert!(!exists("ostrov.test.none"));
    }

    #[test]
    fn the_file_kept_private() {
        let dir = std::env::temp_dir().join(format!("ostrov-secrets-{}", std::process::id()));
        let path = dir.join("secrets.toml");
        file_set(&path, "calendar.password", Some("hunter2")).unwrap();
        file_set(&path, "widget.x.token", Some("t")).unwrap();
        assert_eq!(file_get(&path, "calendar.password").as_deref(), Some("hunter2"));
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        file_set(&path, "calendar.password", None).unwrap();
        assert_eq!(file_get(&path, "calendar.password"), None);
        assert_eq!(file_get(&path, "widget.x.token").as_deref(), Some("t"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
