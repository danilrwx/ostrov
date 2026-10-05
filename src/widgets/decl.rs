//! A widget file read (KDL, v2 or v1) into its widgets' declarations, every node and expression checked as it is
//! read, so a widget once read always renders; and a declaration rendered, its sources' values, the state and
//! its config in scope, into a tree of plugins' nodes (plugins/node.rs) and what its events run.

use std::collections::HashMap;

use kdl::{KdlDocument, KdlNode, KdlValue};
use serde_json::{json, Map, Value};

use super::expr::{self, Scope};
use crate::cc::Show;

/// The nodes drawn as plugins' nodes are, of the same names.
const KINDS: &[&str] =
    &["toggle", "slider", "button", "round", "label", "row", "box", "image", "progress", "chips", "entry", "separator"];
/// What a node's children may say happens to it, an exec each.
const EVENTS: &[&str] = &["click", "toggle", "change"];

/// A node as the file has it: its name, arguments, properties, children, and its line for what is said of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub name: String,
    pub args: Vec<Value>,
    pub props: Map<String, Value>,
    pub children: Vec<Node>,
    pub line: usize,
}

impl Node {
    fn prop(&self, k: &str) -> Option<&str> {
        self.props.get(k).and_then(Value::as_str)
    }

    fn arg(&self) -> Option<&str> {
        self.args.first().and_then(Value::as_str)
    }

    /// Whether it says what happens to its parent: a click, toggle or change with an exec (a toggle without one
    /// is a toggle drawn).
    fn event(&self) -> bool {
        EVENTS.contains(&self.name.as_str()) && self.props.contains_key("exec")
    }
}

/// What a widget is fed by: a command polled every so many seconds, or one left running, a value a line; or one
/// of ostrov's events (events.rs), its last payload.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub name: String,
    pub exec: String,
    /// seconds between polls; None for a listen
    pub every: Option<u64>,
    /// the event followed, for an event source (its exec "")
    pub event: Option<String>,
}

/// One widget a file declares.
#[derive(Clone)]
pub struct Decl {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub sizes: Vec<(u8, u8)>,
    pub sources: Vec<Source>,
    /// what is drawn on the grid: one node its root, more in a column
    pub body: Vec<Node>,
    pub badge: Option<Node>,
    /// when its badge shows unless its panel says otherwise
    pub bar: Show,
    pub settings: Vec<crate::settings::Field>,
}

/// Where a file went wrong: a line (0 when it is not known) and what.
pub type Error = (usize, String);

fn line_at(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())].iter().filter(|b| **b == b'\n').count() + 1
}

fn value(v: &KdlValue) -> Value {
    match v {
        KdlValue::String(s) => json!(s),
        KdlValue::Integer(i) => json!(*i as i64),
        KdlValue::Float(f) => json!(f),
        KdlValue::Bool(b) => json!(b),
        KdlValue::Null => Value::Null,
    }
}

fn node(text: &str, n: &KdlNode) -> Node {
    let mut args = Vec::new();
    let mut props = Map::new();
    for e in n.entries() {
        match e.name() {
            Some(k) => {
                props.insert(k.value().to_string(), value(e.value()));
            }
            None => args.push(value(e.value())),
        }
    }
    Node {
        name: n.name().value().to_string(),
        args,
        props,
        children: n.children().map(|d| d.nodes().iter().map(|c| node(text, c)).collect()).unwrap_or_default(),
        line: line_at(text, n.span().offset()),
    }
}

/// A file's widgets.
pub fn parse(text: &str) -> Result<Vec<Decl>, Error> {
    let doc: KdlDocument = text.parse().map_err(|e: kdl::KdlError| {
        let d = e.diagnostics.first();
        let line = d.map(|d| line_at(text, d.span.offset())).unwrap_or(0);
        let msg = d.and_then(|d| d.message.clone().or(d.help.clone())).unwrap_or_else(|| e.to_string());
        (line, msg)
    })?;
    doc.nodes().iter().map(|n| node(text, n)).map(|n| decl(&n)).collect()
}

fn word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// "4x1 2x1": the sizes, within the grid.
fn sizes(s: &str) -> Option<Vec<(u8, u8)>> {
    s.split_whitespace()
        .map(|wh| {
            let (w, h) = wh.split_once('x')?;
            let (w, h): (u8, u8) = (w.parse().ok()?, h.parse().ok()?);
            ((1..=crate::cc::grid::BASE).contains(&w) && h >= 1).then_some((w, h))
        })
        .collect()
}

fn decl(n: &Node) -> Result<Decl, Error> {
    let err = |n: &Node, m: String| Err((n.line, m));
    if n.name != "widget" {
        return err(n, format!("{:?}: a file declares widgets: widget \"id\" {{ ... }}", n.name));
    }
    let Some(id) = n.arg().filter(|id| word(id)) else {
        return err(n, "widget: its id first, of [a-z0-9-]: widget \"vless\"".into());
    };
    let sizes = match n.prop("sizes") {
        Some(s) => match sizes(s) {
            Some(v) if !v.is_empty() => v,
            _ => return err(n, format!("sizes {s:?}: cells as \"4x1 2x1\", at most 8 wide")),
        },
        None => crate::modules::TOGGLE.to_vec(),
    };
    let mut d = Decl {
        id: id.into(),
        name: n.prop("name").unwrap_or(id).into(),
        icon: n.prop("icon").unwrap_or("application-x-executable-symbolic").into(),
        sizes,
        sources: Vec::new(),
        body: Vec::new(),
        badge: None,
        bar: Show::Never,
        settings: Vec::new(),
    };
    for c in &n.children {
        match c.name.as_str() {
            "event" => {
                let Some(name) = c.arg().filter(|s| word(&s.replace('_', "-"))) else {
                    return err(c, "event: its name first: event \"win\" on=\"window\"".into());
                };
                if ["state", "config", "value"].contains(&name) || d.sources.iter().any(|s| s.name == name) {
                    return err(c, format!("{name:?}: taken"));
                }
                let Some(on) = c.prop("on") else { return err(c, "event: on=\"window\" (an event of ostrov's)".into()) };
                d.sources.push(Source { name: name.into(), exec: String::new(), every: None, event: Some(on.into()) });
            }
            "poll" | "listen" => {
                let Some(name) = c.arg().filter(|s| word(&s.replace('_', "-"))) else {
                    return err(c, format!("{}: its name first: {} \"st\" exec=\"...\"", c.name, c.name));
                };
                if ["state", "config", "value"].contains(&name) || d.sources.iter().any(|s| s.name == name) {
                    return err(c, format!("{name:?}: taken"));
                }
                let Some(exec) = c.prop("exec") else { return err(c, format!("{}: no exec", c.name)) };
                expr::check(exec).map_err(|e| (c.line, e))?;
                let every = match (c.name.as_str(), &c.props.get("every")) {
                    ("listen", _) => None,
                    (_, Some(Value::Number(n))) => n.as_u64().filter(|s| *s > 0),
                    (_, Some(Value::String(s))) => crate::settings::parse_duration(s).filter(|s| *s > 0),
                    _ => None,
                };
                if c.name == "poll" && every.is_none() {
                    return err(c, "poll: every=\"5s\" (or 30, \"10m\", \"1h\")".into());
                }
                d.sources.push(Source { name: name.into(), exec: exec.into(), every, event: None });
            }
            "badge" => {
                check(c, false)?;
                d.bar = match c.prop("show").unwrap_or("always") {
                    "always" => Show::Always,
                    "active" => Show::Active,
                    "never" => Show::Never,
                    s => return err(c, format!("show {s:?}: always, active or never")),
                };
                d.badge = Some(c.clone());
            }
            "setting" => d.settings.push(setting(c)?),
            _ => {
                check(c, false)?;
                d.body.push(c.clone());
            }
        }
    }
    Ok(d)
}

/// `setting "key" type=... default=... title=...`: a field of the Settings page's schema; options="a b" a list.
fn setting(c: &Node) -> Result<crate::settings::Field, Error> {
    let Some(key) = c.arg() else {
        return Err((c.line, "setting: its key first: setting \"host\" type=\"string\"".into()));
    };
    let mut f = c.props.clone();
    f.insert("key".into(), json!(key));
    f.entry("title").or_insert(json!(key));
    f.entry("type").or_insert(json!("string"));
    if let Some(Value::String(o)) = f.get("options") {
        let list: Vec<&str> = o.split_whitespace().collect();
        f.insert("options".into(), json!(list));
    }
    serde_json::from_value(Value::Object(f)).map_err(|e| (c.line, format!("setting {key:?}: {e}")))
}

/// A node and its children known, their expressions well formed. menu only under a toggle.
fn check(n: &Node, in_toggle: bool) -> Result<(), Error> {
    let err = |m: String| Err((n.line, m));
    for v in n.args.iter().chain(n.props.values()) {
        if let Value::String(s) = v {
            expr::check(s).map_err(|e| (n.line, e))?;
        }
    }
    match n.name.as_str() {
        "for" if n.arg().is_none() || n.prop("in").is_none() => {
            return err("for: for \"x\" in=\"{list}\" { ... }".into());
        }
        "if" if n.args.is_empty() => return err("if: if \"{cond}\" { ... }".into()),
        "menu" if !in_toggle => return err("menu: only in a toggle".into()),
        _ if n.event() => return Ok(()),
        e @ ("click" | "change") => return err(format!("{e}: no exec")),
        "for" | "if" | "menu" | "badge" => {}
        k if KINDS.contains(&k) => {}
        k => return err(format!("{k:?}: not a node; the nodes: {}, for, if", KINDS.join(", "))),
    }
    n.children.iter().try_for_each(|c| check(c, n.name == "toggle"))
}

/// A node's events, by its id in the tree drawn: (event, exec) each, and the loop variables they see.
pub struct Handler {
    pub events: Vec<(String, String)>,
    pub locals: Vec<(String, Value)>,
}

pub type Handlers = HashMap<String, Handler>;

fn string(t: &str, sc: &Scope) -> Value {
    expr::interp(t, sc, false).unwrap_or(Value::Null)
}

/// A property's value as its field takes it: on a switch (a chip's index), value a number, options a list,
/// the rest text.
fn prop(kind: &str, k: &str, v: &Value, sc: &Scope) -> Value {
    let v = match v {
        Value::String(t) => string(t, sc),
        v => v.clone(),
    };
    match (kind, k) {
        ("chips", "on") => v.as_f64().map(|n| json!(n as u64)).unwrap_or(Value::Null),
        (_, "on") => json!(expr::truthy(&v)),
        (_, "value") => json!(v.as_f64().or_else(|| expr::text(&v).trim().parse().ok()).unwrap_or(0.0)),
        (_, "options") => match &v {
            Value::Array(a) => json!(a.iter().map(expr::text).collect::<Vec<_>>()),
            v => json!(expr::text(v).split_whitespace().collect::<Vec<_>>()),
        },
        _ => json!(expr::text(&v)),
    }
}

/// Nodes rendered into plugins' nodes: a for's once for each item, an if's when it holds.
pub fn render(nodes: &[Node], sc: &mut Scope, hs: &mut Handlers) -> Vec<Value> {
    let mut out = Vec::new();
    for n in nodes {
        match n.name.as_str() {
            "for" => {
                let var = n.arg().unwrap_or("it").to_string();
                let list = match string(n.prop("in").unwrap_or(""), sc) {
                    Value::Array(a) => a,
                    Value::Object(m) => m.into_iter().map(|(_, v)| v).collect(),
                    Value::Null => Vec::new(),
                    v => vec![v],
                };
                for item in list {
                    sc.locals.push((var.clone(), item));
                    out.extend(render(&n.children, sc, hs));
                    sc.locals.pop();
                }
            }
            "if" => {
                let cond = match &n.args[0] {
                    Value::String(t) => string(t, sc),
                    v => v.clone(),
                };
                if expr::truthy(&cond) {
                    out.extend(render(&n.children, sc, hs));
                }
            }
            _ if n.event() || n.name == "menu" => {}
            kind => out.push(element(kind, n, sc, hs)),
        }
    }
    out
}

/// Several nodes as one: the one, or a column of them.
pub fn one(mut els: Vec<Value>) -> Value {
    if els.len() == 1 { els.remove(0) } else { json!({"type": "box", "children": els}) }
}

fn element(kind: &str, n: &Node, sc: &mut Scope, hs: &mut Handlers) -> Value {
    let mut el = Map::new();
    el.insert("type".into(), json!(kind));
    for (k, v) in &n.props {
        el.insert(k.clone(), prop(kind, k, v, sc));
    }
    let events: Vec<(String, String)> = n
        .children
        .iter()
        .filter(|c| c.event())
        .map(|c| (c.name.clone(), c.prop("exec").unwrap_or("").to_string()))
        .collect();
    if !events.is_empty() {
        let id = hs.len().to_string();
        hs.insert(id.clone(), Handler { events, locals: sc.locals.clone() });
        el.insert("id".into(), json!(id));
    }
    if let Some(m) = n.children.iter().find(|c| c.name == "menu") {
        el.insert("menu".into(), one(render(&m.children, sc, hs)));
    }
    if kind == "box" {
        el.insert("children".into(), json!(render(&n.children, sc, hs)));
    }
    Value::Object(el)
}

/// A badge rendered: its icon and words in a row, and whether it is active (true if it does not say).
pub fn badge(b: &Node, sc: &Scope) -> (Value, bool) {
    let icon = b.prop("icon").map(|t| expr::text(&string(t, sc))).unwrap_or_default();
    let text = b.prop("text").map(|t| expr::text(&string(t, sc))).unwrap_or_default();
    let mut children = Vec::new();
    if !icon.is_empty() {
        children.push(json!({"type": "image", "icon": icon}));
    }
    if !text.is_empty() {
        children.push(json!({"type": "label", "text": text}));
    }
    let active = match b.props.get("active") {
        Some(Value::String(t)) => expr::truthy(&string(t, sc)),
        Some(v) => expr::truthy(v),
        None => true,
    };
    (json!({"type": "box", "orientation": "horizontal", "children": children}), active)
}

/// Whether an event fired matches a handler's: a click on a toggle is its toggle.
pub fn matches(handler: &str, fired: &str) -> bool {
    handler == fired || handler == "click" && fired == "toggle"
}

#[cfg(test)]
mod tests {
    use super::*;

    const VLESS: &str = r#"
widget "vless" name="VLESS" icon="network-vpn-symbolic" sizes="4x1 2x1 1x1" {
    poll "st" every="5s" exec="vless status --json"
    toggle icon="network-vpn-symbolic" title="VLESS" sub="{st.profile}" on="{st.on}" {
        click exec="vless toggle"
        menu {
            for "p" in="{st.profiles}" {
                row text="{p}" on="{p == st.profile}" { click exec="vless profile {p}" }
            }
        }
    }
    badge icon="network-vpn-symbolic" text="{st.profile}" show="active" active="{st.on}"
    setting "host" type="string" default="example.org"
    setting "mode" type="choice" options="tcp grpc" default="tcp"
}
"#;

    fn scope(st: Value) -> Map<String, Value> {
        let Value::Object(m) = json!({ "st": st }) else { unreachable!() };
        m
    }

    #[test]
    fn reads_the_example() {
        let ds = parse(VLESS).unwrap();
        assert_eq!(ds.len(), 1);
        let d = &ds[0];
        assert_eq!((d.id.as_str(), d.name.as_str()), ("vless", "VLESS"));
        assert_eq!(d.sizes, [(4, 1), (2, 1), (1, 1)]);
        assert_eq!(d.sources, [Source { name: "st".into(), exec: "vless status --json".into(), every: Some(5), event: None }]);
        assert_eq!(d.bar, Show::Active);
        assert_eq!(d.body.len(), 1);
        assert_eq!(d.body[0].line, 4);
        assert_eq!(d.settings.len(), 2);
        assert_eq!(d.settings[1].kind, crate::settings::Kind::Choice { options: vec![
            crate::settings::Opt::Plain("tcp".into()),
            crate::settings::Opt::Plain("grpc".into()),
        ] });
    }

    #[test]
    fn the_examples_read() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/widgets");
        let mut n = 0;
        for f in std::fs::read_dir(dir).unwrap().flatten() {
            let ds = parse(&std::fs::read_to_string(f.path()).unwrap()).unwrap_or_else(|e| panic!("{f:?}: {e:?}"));
            n += ds.len();
            if let Some(src) = ds[0].sources.iter().find(|s| s.name == "track") {
                let g = Map::new();
                let cmd = expr::interp(&src.exec, &Scope { globals: &g, locals: Vec::new() }, true).unwrap();
                assert_eq!(cmd, json!("playerctl --follow metadata --format '{{artist}} – {{title}}'"));
            }
        }
        assert_eq!(n, 3);
    }

    #[test]
    fn reads_v1_too() {
        let ds = parse("widget \"x\" {\n  label text=\"hi\" class=\"dim\"\n  toggle on=true\n}").unwrap();
        assert_eq!(ds[0].body[1].props["on"], json!(true));
        assert_eq!(ds[0].sizes, crate::modules::TOGGLE);
    }

    #[test]
    fn says_where_it_is_wrong() {
        let bad = |t: &str| parse(t).err().unwrap_or((0, "read".into()));
        assert_eq!(bad("widget \"a\" {\n  label text=\"{x ==}\"\n}").0, 2);
        assert!(bad("widget \"a\" {\n  hologram\n}").1.contains("hologram"));
        assert!(bad("widget \"A\"").1.contains("id"));
        assert!(bad("thing \"a\"").1.contains("widget"));
        assert!(bad("widget \"a\" sizes=\"9x1\"").1.contains("sizes"));
        assert!(bad("widget \"a\" {\n event \"w\"\n}").1.contains("on="));
        assert!(bad("widget \"a\" {\n poll \"s\" exec=\"x\"\n}").1.contains("every"));
        assert!(bad("widget \"a\" {\n poll \"state\" every=1 exec=\"x\"\n}").1.contains("taken"));
        assert!(bad("widget \"a\" {\n label {\n  menu\n }\n}").1.contains("menu"));
        assert!(bad("widget \"a\" {\n button {\n  click\n }\n}").1.contains("exec"));
        assert!(bad("widget \"a\" {\n badge show=\"often\"\n}").1.contains("often"));
        assert!(bad("widget \"a\" {\n setting \"n\" type=\"number\"\n}").1.contains("setting"));
        assert_eq!(bad("widget \"a\" {\n\n  label text=\"x\n").0, 3);
    }

    #[test]
    fn renders_a_tree() {
        let d = &parse(VLESS).unwrap()[0];
        let g = scope(json!({"on": true, "profile": "work", "profiles": ["home", "work"]}));
        let mut sc = Scope { globals: &g, locals: Vec::new() };
        let mut hs = Handlers::new();
        let tree = one(render(&d.body, &mut sc, &mut hs));
        assert_eq!(tree, json!({
            "type": "toggle", "id": "0", "icon": "network-vpn-symbolic", "title": "VLESS", "sub": "work", "on": true,
            "menu": {"type": "box", "children": [
                {"type": "row", "id": "1", "text": "home", "on": false},
                {"type": "row", "id": "2", "text": "work", "on": true},
            ]}
        }));
        assert_eq!(hs["0"].events, [("click".to_string(), "vless toggle".to_string())]);
        assert_eq!(hs["1"].locals, [("p".to_string(), json!("home"))]);
        // and drawn as plugins' nodes are
        assert!(serde_json::from_value::<crate::plugins::node::El>(tree).is_ok());
        let (b, active) = badge(d.badge.as_ref().unwrap(), &sc);
        assert!(active);
        assert_eq!(b["children"][1], json!({"type": "label", "text": "work"}));
        let g = scope(json!({"on": "", "profile": 3}));
        let (_, active) = badge(d.badge.as_ref().unwrap(), &Scope { globals: &g, locals: Vec::new() });
        assert!(!active);
    }

    #[test]
    fn coerces_and_branches() {
        let d = &parse(
            r#"widget "c" {
                if "{st.n == 2}" { slider value="{st.v}" }
                if "{st.missing}" { label text="never" }
                label text="{st.n}"
                chips options="{st.opts}" on="{st.n}"
                chips options="x y"
            }"#,
        )
        .unwrap()[0];
        let g = scope(json!({"n": 2, "v": "0.25", "opts": ["a", 1]}));
        let mut sc = Scope { globals: &g, locals: Vec::new() };
        let tree = one(render(&d.body, &mut sc, &mut Handlers::new()));
        assert_eq!(tree["children"][0], json!({"type": "slider", "value": 0.25}));
        assert_eq!(tree["children"][1], json!({"type": "label", "text": "2"}));
        assert_eq!(tree["children"][2], json!({"type": "chips", "options": ["a", "1"], "on": 2}));
        assert_eq!(tree["children"][3]["options"], json!(["x", "y"]));
        assert!(matches("click", "toggle") && matches("change", "change") && !matches("toggle", "click"));
    }
}
