//! The launcher's calculator: arithmetic typed in the prompt evaluated by a small recursive descent over
//! + - * / % ^ (right-associative, binding tighter than a unary minus, so -2^2 is -4), parentheses, sqrt and pi.
//! A decimal comma reads as a point. A plain number is not an expression: the launcher would show it back as is.

/// The value of what is typed, shown; None when it is no expression (an app's name, a lone number) or no finite
/// value (a division by zero).
pub fn eval(s: &str) -> Option<String> {
    let s = s.trim().replace(',', ".");
    if s.is_empty() || s.parse::<f64>().is_ok() {
        return None;
    }
    let mut p = Parser { s: s.as_bytes(), at: 0 };
    let x = p.expr()?;
    p.skip();
    if p.at != p.s.len() || !x.is_finite() {
        return None;
    }
    // twelve decimals at most: 0.1+0.2 is 0.3, not 0.30000000000000004
    let t = format!("{:.12}", x + 0.0);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    Some(if t == "-0" { "0".into() } else { t.into() })
}

struct Parser<'a> {
    s: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn skip(&mut self) {
        while self.s.get(self.at).is_some_and(|c| c.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    /// The next byte, past spaces, taken when it is c.
    fn eat(&mut self, c: u8) -> bool {
        self.skip();
        let hit = self.s.get(self.at) == Some(&c);
        self.at += hit as usize;
        hit
    }

    fn word(&mut self, w: &str) -> bool {
        self.skip();
        let hit = self.s[self.at..].starts_with(w.as_bytes());
        self.at += if hit { w.len() } else { 0 };
        hit
    }

    fn expr(&mut self) -> Option<f64> {
        let mut x = self.term()?;
        loop {
            if self.eat(b'+') {
                x += self.term()?;
            } else if self.eat(b'-') {
                x -= self.term()?;
            } else {
                return Some(x);
            }
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut x = self.unary()?;
        loop {
            if self.eat(b'*') {
                x *= self.unary()?;
            } else if self.eat(b'/') {
                x /= self.unary()?;
            } else if self.eat(b'%') {
                x %= self.unary()?;
            } else {
                return Some(x);
            }
        }
    }

    fn unary(&mut self) -> Option<f64> {
        if self.eat(b'-') {
            return Some(-self.unary()?);
        }
        if self.eat(b'+') {
            return self.unary();
        }
        let x = self.atom()?;
        if self.eat(b'^') {
            return Some(x.powf(self.unary()?));
        }
        Some(x)
    }

    fn atom(&mut self) -> Option<f64> {
        if self.eat(b'(') {
            let x = self.expr()?;
            return self.eat(b')').then_some(x);
        }
        if self.word("pi") {
            return Some(std::f64::consts::PI);
        }
        if self.word("sqrt") {
            return Some(self.atom()?.sqrt());
        }
        self.skip();
        let start = self.at;
        while self.s.get(self.at).is_some_and(|c| c.is_ascii_digit() || *c == b'.') {
            self.at += 1;
        }
        std::str::from_utf8(&self.s[start..self.at]).ok()?.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::eval;

    #[test]
    fn arithmetic() {
        assert_eq!(eval("1+2*3").as_deref(), Some("7"));
        assert_eq!(eval("(1+2)*3").as_deref(), Some("9"));
        assert_eq!(eval("2^3^2").as_deref(), Some("512"));
        assert_eq!(eval("-2^2").as_deref(), Some("-4"));
        assert_eq!(eval("10 % 4 - -1").as_deref(), Some("3"));
        assert_eq!(eval("0.1+0.2").as_deref(), Some("0.3"));
        assert_eq!(eval("1,5 * 2").as_deref(), Some("3"));
        assert_eq!(eval("7/2").as_deref(), Some("3.5"));
        assert_eq!(eval("sqrt(16) + sqrt 9").as_deref(), Some("7"));
        assert_eq!(eval("2*pi").as_deref(), Some("6.28318530718"));
        assert_eq!(eval("-(3-3)").as_deref(), Some("0"));
    }

    #[test]
    fn not_expressions() {
        for s in ["", "42", "-5", "3,14", "firefox", "1+", "(1", "1/0", "2 3", "1..2+1", "sqrt(-1)"] {
            assert_eq!(eval(s), None, "{s}");
        }
    }
}
