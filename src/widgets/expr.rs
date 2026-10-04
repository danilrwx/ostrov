//! The expressions of a widget file, between braces in its strings: a path into what is known (`st.profile`,
//! `list.0`), string and number literals, `==`, `!=`, `!`, `&&`, `||` and parentheses. `a || b` is a if it is
//! true, else b, so `{st.name || 'none'}` gives a default. A path to nothing is null, never an error; only a
//! malformed expression is one, found as the file is read.

use serde_json::{Map, Value};

/// What an expression sees: the sources, the state, the config (global), and the loop variables and an event's
/// value over them (local, the innermost last).
pub struct Scope<'a> {
    pub globals: &'a Map<String, Value>,
    pub locals: Vec<(String, Value)>,
}

impl Scope<'_> {
    fn root(&self, name: &str) -> Option<&Value> {
        self.locals.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v).or_else(|| self.globals.get(name))
    }
}

/// Whether a value counts as true: not null, false, 0, "" or an empty list.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

/// A value as text: a string as it is, null as nothing, anything else as JSON.
pub fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// s as one word for sh, whatever it holds: in single quotes, its own single quotes closed and escaped.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::String(_), Value::String(_)) | (Value::Bool(_), Value::Bool(_)) | (Value::Null, Value::Null) => a == b,
        // of two kinds ("1" and 1): as text
        _ => text(a) == text(b),
    }
}

/// One expression evaluated.
pub fn eval(src: &str, scope: &Scope) -> Result<Value, String> {
    let mut p = Parser { src, s: src.chars().collect(), i: 0, scope };
    let v = p.or()?;
    p.ws();
    match p.s.get(p.i) {
        None => Ok(v),
        Some(c) => Err(format!("{c:?} unexpected in {{{src}}}")),
    }
}

/// A string with expressions in braces ({{ and }} are braces themselves): one exactly "{expr}" its value, of
/// whatever type; anything else the text with each value put in, each quoted for sh if shell says so.
pub fn interp(t: &str, scope: &Scope, shell: bool) -> Result<Value, String> {
    let parts = split(t)?;
    if let [Part::Expr(e)] = parts.as_slice()
        && !shell
    {
        return eval(e, scope);
    }
    let mut out = String::new();
    for p in parts {
        match p {
            Part::Text(s) => out += &s,
            Part::Expr(e) => {
                let v = text(&eval(&e, scope)?);
                out += &if shell { quote(&v) } else { v };
            }
        }
    }
    Ok(Value::String(out))
}

/// Whether a string's expressions are well formed, nothing looked up.
pub fn check(t: &str) -> Result<(), String> {
    let globals = Map::new();
    interp(t, &Scope { globals: &globals, locals: Vec::new() }, false).map(|_| ())
}

#[derive(Debug, PartialEq)]
enum Part {
    Text(String),
    Expr(String),
}

/// A template cut at its braces, a brace inside an expression's quotes not counting.
fn split(t: &str) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    let mut lit = String::new();
    let mut cs = t.chars().peekable();
    while let Some(c) = cs.next() {
        match c {
            '{' if cs.peek() == Some(&'{') => {
                cs.next();
                lit.push('{');
            }
            '}' if cs.peek() == Some(&'}') => {
                cs.next();
                lit.push('}');
            }
            '}' => return Err(format!("a lone }} in {t:?}")),
            '{' => {
                let mut e = String::new();
                let mut quote = None;
                loop {
                    match (cs.next(), quote) {
                        (None, _) => return Err(format!("a {{ not closed in {t:?}")),
                        (Some('}'), None) => break,
                        (Some(c @ ('\'' | '"')), None) => {
                            quote = Some(c);
                            e.push(c);
                        }
                        (Some(c), Some(q)) if c == q => {
                            quote = None;
                            e.push(c);
                        }
                        (Some(c), _) => e.push(c),
                    }
                }
                if !lit.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut lit)));
                }
                parts.push(Part::Expr(e));
            }
            c => lit.push(c),
        }
    }
    if !lit.is_empty() || parts.is_empty() {
        parts.push(Part::Text(lit));
    }
    Ok(parts)
}

struct Parser<'a> {
    src: &'a str,
    s: Vec<char>,
    i: usize,
    scope: &'a Scope<'a>,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn eat(&mut self, op: &str) -> bool {
        self.ws();
        let n = op.chars().count();
        if self.s.get(self.i..self.i + n).is_some_and(|w| w.iter().copied().eq(op.chars())) {
            self.i += n;
            return true;
        }
        false
    }

    fn or(&mut self) -> Result<Value, String> {
        let mut v = self.and()?;
        while self.eat("||") {
            let r = self.and()?;
            if !truthy(&v) {
                v = r;
            }
        }
        Ok(v)
    }

    fn and(&mut self) -> Result<Value, String> {
        let mut v = self.cmp()?;
        while self.eat("&&") {
            let r = self.cmp()?;
            if truthy(&v) {
                v = r;
            }
        }
        Ok(v)
    }

    fn cmp(&mut self) -> Result<Value, String> {
        let l = self.unary()?;
        if self.eat("==") {
            return Ok(Value::Bool(equal(&l, &self.unary()?)));
        }
        if self.eat("!=") {
            return Ok(Value::Bool(!equal(&l, &self.unary()?)));
        }
        Ok(l)
    }

    fn unary(&mut self) -> Result<Value, String> {
        if self.eat("!") {
            return Ok(Value::Bool(!truthy(&self.unary()?)));
        }
        self.primary()
    }

    fn word(&mut self) -> String {
        let start = self.i;
        while self.s.get(self.i).is_some_and(|c| c.is_alphanumeric() || *c == '_' || *c == '-') {
            self.i += 1;
        }
        self.s[start..self.i].iter().collect()
    }

    fn primary(&mut self) -> Result<Value, String> {
        self.ws();
        match self.s.get(self.i).copied() {
            None => Err(format!("an expression ends early: {{{}}}", self.src)),
            Some('(') => {
                self.i += 1;
                let v = self.or()?;
                if !self.eat(")") {
                    return Err(format!("a ( not closed in {{{}}}", self.src));
                }
                Ok(v)
            }
            Some(q @ ('\'' | '"')) => {
                self.i += 1;
                let start = self.i;
                while self.s.get(self.i).is_some_and(|c| *c != q) {
                    self.i += 1;
                }
                if self.i == self.s.len() {
                    return Err(format!("a string not closed in {{{}}}", self.src));
                }
                let s: String = self.s[start..self.i].iter().collect();
                self.i += 1;
                Ok(Value::String(s))
            }
            Some(c) if c.is_ascii_digit() || c == '-' => {
                let start = self.i;
                self.i += 1;
                while self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || *c == '.') {
                    self.i += 1;
                }
                let n: String = self.s[start..self.i].iter().collect();
                let f: f64 = n.parse().map_err(|_| format!("not a number: {n} in {{{}}}", self.src))?;
                Ok(serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null))
            }
            Some(c) if c.is_alphabetic() || c == '_' => {
                let first = self.word();
                let mut v = match first.as_str() {
                    "true" => return Ok(Value::Bool(true)),
                    "false" => return Ok(Value::Bool(false)),
                    "null" => return Ok(Value::Null),
                    _ => self.scope.root(&first).cloned().unwrap_or(Value::Null),
                };
                while self.s.get(self.i) == Some(&'.') {
                    self.i += 1;
                    let seg = self.word();
                    if seg.is_empty() {
                        return Err(format!("a path ends in . in {{{}}}", self.src));
                    }
                    v = match &v {
                        Value::Object(m) => m.get(&seg).cloned().unwrap_or(Value::Null),
                        Value::Array(a) => {
                            seg.parse::<usize>().ok().and_then(|i| a.get(i)).cloned().unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    };
                }
                Ok(v)
            }
            Some(c) => Err(format!("{c:?} unexpected in {{{}}}", self.src)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn with<T>(f: impl FnOnce(&Scope) -> T) -> T {
        let g = json!({"st": {"on": true, "profile": "work", "profiles": ["home", "work"], "n": 3, "empty": ""}});
        let Value::Object(globals) = g else { unreachable!() };
        f(&Scope { globals: &globals, locals: vec![("p".into(), json!("work"))] })
    }

    fn ev(s: &str) -> Value {
        with(|sc| eval(s, sc).unwrap())
    }

    #[test]
    fn paths_and_literals() {
        assert_eq!(ev("st.profile"), json!("work"));
        assert_eq!(ev("st.profiles.1"), json!("work"));
        assert_eq!(ev("st.profiles.7"), Value::Null);
        assert_eq!(ev("st.nothing.deeper"), Value::Null);
        assert_eq!(ev("nobody"), Value::Null);
        assert_eq!(ev("'a b'"), json!("a b"));
        assert_eq!(ev("\"x\""), json!("x"));
        assert_eq!(ev("4.5"), json!(4.5));
        assert_eq!(ev("p"), json!("work"));
    }

    #[test]
    fn operators() {
        assert_eq!(ev("p == st.profile"), json!(true));
        assert_eq!(ev("p != 'home'"), json!(true));
        assert_eq!(ev("st.n == 3"), json!(true));
        assert_eq!(ev("st.n == '3'"), json!(true));
        assert_eq!(ev("!st.on"), json!(false));
        assert_eq!(ev("!st.empty"), json!(true));
        assert_eq!(ev("st.on && p == 'work'"), json!(true));
        assert_eq!(ev("!(st.on && false) || false"), json!(true));
        assert_eq!(ev("st.empty || 'none'"), json!("none"));
        assert_eq!(ev("st.on && st.profile"), json!("work"));
    }

    #[test]
    fn errors() {
        with(|sc| {
            for bad in ["", "a ==", "(a", "'open", "a.", "a b", "a = b", "#"] {
                assert!(eval(bad, sc).is_err(), "{bad}");
            }
        });
        assert!(check("{a").is_err());
        assert!(check("a}").is_err());
        assert!(check("{a == }").is_err());
        assert!(check("{{literal}} {a.b} {'}'}").is_ok());
    }

    #[test]
    fn interpolation() {
        with(|sc| {
            assert_eq!(interp("{st.on}", sc, false).unwrap(), json!(true));
            assert_eq!(interp("{st.profiles}", sc, false).unwrap(), json!(["home", "work"]));
            assert_eq!(interp(" {st.on}", sc, false).unwrap(), json!(" true"));
            assert_eq!(interp("{st.n} of {p}", sc, false).unwrap(), json!("3 of work"));
            assert_eq!(interp("plain", sc, false).unwrap(), json!("plain"));
            assert_eq!(interp("", sc, false).unwrap(), json!(""));
            assert_eq!(interp("{{x}}", sc, false).unwrap(), json!("{x}"));
        });
    }

    #[test]
    fn shell_quoting() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote("$(rm -rf ~)`x`;\"q\""), "'$(rm -rf ~)`x`;\"q\"'");
        let g = Map::new();
        let sc = Scope { globals: &g, locals: vec![("p".into(), json!("x'; rm -rf ~; echo '"))] };
        let cmd = interp("vless profile {p}", &sc, true).unwrap();
        assert_eq!(cmd, json!(r"vless profile 'x'\''; rm -rf ~; echo '\'''"));
        // even a whole-string expression is quoted text in a command
        assert_eq!(interp("{p}", &sc, true).unwrap(), json!(r"'x'\''; rm -rf ~; echo '\'''"));
        // and it reads back as the one word it was
        let out = std::process::Command::new("sh").args(["-c", &format!("printf %s {}", quote("a'b $c"))]).output();
        assert_eq!(out.map(|o| o.stdout).unwrap_or_default(), b"a'b $c");
    }
}
