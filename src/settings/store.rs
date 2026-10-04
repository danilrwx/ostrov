//! config.toml edited in place: a key set or taken out, every other line (the user's comments, blank lines, the
//! order of things) as it was, the value's own trailing comment kept too.

use serde_json::Value;
use toml_edit::{DocumentMut, Item, Table};

/// The text with key of table ("calendar", "widget.wallpaper") set to v, or taken out for None. A table not
/// there is made, its parents implicit ([widget.wallpaper] alone, no [widget]). A number goes in as an integer
/// when integer says so, else as a float.
pub fn set(text: &str, table: &str, key: &str, v: Option<&Value>, integer: bool) -> Result<String, String> {
    let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
    let mut t: &mut Table = doc.as_table_mut();
    for part in table.split('.').filter(|p| !p.is_empty()) {
        let item = t.entry(part).or_insert_with(|| {
            let mut n = Table::new();
            n.set_implicit(true);
            Item::Table(n)
        });
        t = item.as_table_mut().ok_or(format!("{table} is not a table in the config"))?;
    }
    match v {
        None => drop(t.remove(key)),
        Some(v) => {
            let mut new = to_toml(v, integer)?;
            match t.get_mut(key).and_then(Item::as_value_mut) {
                Some(old) => {
                    *new.decor_mut() = old.decor().clone();
                    *old = new;
                }
                None => {
                    t.set_implicit(false);
                    t.insert(key, Item::Value(new));
                }
            }
        }
    }
    Ok(doc.to_string())
}

fn to_toml(v: &Value, integer: bool) -> Result<toml_edit::Value, String> {
    Ok(match v {
        Value::String(s) => s.as_str().into(),
        Value::Bool(b) => (*b).into(),
        Value::Number(n) if integer => n.as_i64().unwrap_or_else(|| n.as_f64().unwrap_or(0.0).round() as i64).into(),
        Value::Number(n) => n.as_f64().unwrap_or(0.0).into(),
        Value::Array(a) => {
            let mut arr = toml_edit::Array::new();
            for x in a {
                arr.push(to_toml(x, integer)?);
            }
            arr.into()
        }
        _ => return Err(format!("{v} has no TOML form")),
    })
}

/// The value at table's key in the text, as JSON; None when not there.
pub fn get(text: &str, table: &str, key: &str) -> Option<Value> {
    let doc: toml::Table = text.parse().ok()?;
    let mut t = &doc;
    for part in table.split('.').filter(|p| !p.is_empty()) {
        t = t.get(part)?.as_table()?;
    }
    serde_json::to_value(t.get(key)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEXT: &str = "# ostrov's config\n\n# the bar's blocks\n[bar]\nleft = [\"workspaces\"]  # the left\n\n\
                        # seconds\n[idle]\nlock = 600          # to the lock\nscreens_off = 900\n";

    #[test]
    fn comments_and_order_kept() {
        let out = set(TEXT, "idle", "lock", Some(&json!(300.0)), true).unwrap();
        assert_eq!(out, TEXT.replace("lock = 600", "lock = 300"));
        let out = set(&out, "bar", "left", Some(&json!(["workspaces", "clock"])), false).unwrap();
        assert!(out.contains("left = [\"workspaces\", \"clock\"]  # the left\n"), "{out}");
        assert!(out.starts_with("# ostrov's config\n\n# the bar's blocks\n[bar]\n"));
    }

    #[test]
    fn types_right() {
        let out = set(TEXT, "appearance", "opacity", Some(&json!(0.8)), false).unwrap();
        let out = set(&out, "appearance", "radius", Some(&json!(12.0)), true).unwrap();
        let out = set(&out, "appearance", "blur", Some(&json!(true)), false).unwrap();
        let out = set(&out, "appearance", "theme", Some(&json!("nord")), false).unwrap();
        assert!(out.ends_with("[appearance]\nopacity = 0.8\nradius = 12\nblur = true\ntheme = \"nord\"\n"), "{out}");
        assert_eq!(get(&out, "appearance", "radius"), Some(json!(12)));
        assert_eq!(get(&out, "idle", "lock"), Some(json!(600)));
        assert_eq!(get(&out, "idle", "nothing"), None);
    }

    #[test]
    fn nested_tables_and_removal() {
        let out = set(TEXT, "widget.wallpaper", "dir", Some(&json!("~/walls")), false).unwrap();
        assert!(out.ends_with("\n[widget.wallpaper]\ndir = \"~/walls\"\n"), "{out}");
        assert!(!out.contains("[widget]\n"));
        assert_eq!(get(&out, "widget.wallpaper", "dir"), Some(json!("~/walls")));
        let out = set(&out, "idle", "screens_off", None, false).unwrap();
        assert!(!out.contains("screens_off") && out.contains("lock = 600          # to the lock\n"));
        assert!(set("lock = 1", "lock", "x", Some(&json!(1)), true).is_err());
    }
}
