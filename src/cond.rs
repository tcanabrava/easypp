//! Just enough of the preprocessor to skip includes in blocks that are
//! certainly off on this platform (`#ifdef _WIN32`, `#if 0`, ...).
//!
//! Conditions are evaluated with three values. A macro known to be defined on
//! Linux with clang is true, one known not to be is false, anything else is
//! unknown. Only a block that is *certainly* false is skipped; unknown blocks
//! are scanned as before, so header guards and feature macros keep working.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Truth {
    True,
    False,
    Unknown,
}

impl Truth {
    fn not(self) -> Truth {
        match self {
            Truth::True => Truth::False,
            Truth::False => Truth::True,
            Truth::Unknown => Truth::Unknown,
        }
    }

    fn and(self, other: Truth) -> Truth {
        match (self, other) {
            (Truth::False, _) | (_, Truth::False) => Truth::False,
            (Truth::True, Truth::True) => Truth::True,
            _ => Truth::Unknown,
        }
    }

    fn or(self, other: Truth) -> Truth {
        match (self, other) {
            (Truth::True, _) | (_, Truth::True) => Truth::True,
            (Truth::False, Truth::False) => Truth::False,
            _ => Truth::Unknown,
        }
    }
}

/// Macros that are certainly defined when building with clang on Linux.
const DEFINED: &[&str] = &[
    "__linux__",
    "__linux",
    "__unix__",
    "__unix",
    "__ELF__",
    "__clang__",
    "__GNUC__",
];

/// Macros that are certainly not defined there (other platforms and compilers).
const UNDEFINED: &[&str] = &[
    "_WIN32",
    "_WIN64",
    "WIN32",
    "_MSC_VER",
    "__MINGW32__",
    "__MINGW64__",
    "__CYGWIN__",
    "__APPLE__",
    "__MACH__",
    "__FreeBSD__",
    "__OpenBSD__",
    "__NetBSD__",
    "__DragonFly__",
    "__ANDROID__",
    "__EMSCRIPTEN__",
    "__HAIKU__",
    "__sun",
    "_AIX",
];

fn defined(name: &str) -> Truth {
    if DEFINED.contains(&name) {
        Truth::True
    } else if UNDEFINED.contains(&name) {
        Truth::False
    } else {
        Truth::Unknown
    }
}

/// Evaluates an `#if` expression: `defined(X)`, `defined X`, `!`, `&&`, `||`,
/// parentheses, integer literals and macro names. Anything else (comparisons,
/// arithmetic) makes the whole expression unknown.
pub fn eval(expr: &str) -> Truth {
    let tokens = tokenize(expr);
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
    };
    match parser.or() {
        Some(value) if parser.pos == tokens.len() => value,
        _ => Truth::Unknown,
    }
}

/// `#ifdef X` / `#ifndef X`.
pub fn ifdef(name: &str, negate: bool) -> Truth {
    let value = defined(name.split_whitespace().next().unwrap_or(""));
    if negate { value.not() } else { value }
}

fn tokenize(expr: &str) -> Vec<String> {
    // Drop a trailing // comment; /* */ comments are rare on #if lines.
    let expr = expr.split("//").next().unwrap_or("");
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c.is_ascii_alphanumeric() || c == '_' {
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    word.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(word);
        } else {
            chars.next();
            let two = chars.peek().map(|&n| format!("{c}{n}"));
            match two.as_deref() {
                Some("&&" | "||" | "==" | "!=" | "<=" | ">=") => {
                    tokens.push(two.unwrap());
                    chars.next();
                }
                _ => tokens.push(c.to_string()),
            }
        }
    }
    tokens
}

struct Parser<'a> {
    tokens: &'a [String],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.pos).map(String::as_str)
    }

    fn eat(&mut self, token: &str) -> bool {
        if self.peek() == Some(token) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn or(&mut self) -> Option<Truth> {
        let mut value = self.and()?;
        while self.eat("||") {
            value = value.or(self.and()?);
        }
        Some(value)
    }

    fn and(&mut self) -> Option<Truth> {
        let mut value = self.unary()?;
        while self.eat("&&") {
            value = value.and(self.unary()?);
        }
        Some(value)
    }

    fn unary(&mut self) -> Option<Truth> {
        if self.eat("!") {
            return Some(self.unary()?.not());
        }
        if self.eat("(") {
            let value = self.or()?;
            return self.eat(")").then_some(value);
        }
        let token = self.peek()?.to_string();
        self.pos += 1;
        if token == "defined" {
            let paren = self.eat("(");
            let name = self.peek()?.to_string();
            self.pos += 1;
            if paren && !self.eat(")") {
                return None;
            }
            return Some(defined(&name));
        }
        if let Ok(number) = token.trim_end_matches(['L', 'l', 'U', 'u']).parse::<i64>() {
            return Some(if number != 0 {
                Truth::True
            } else {
                Truth::False
            });
        }
        if token.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            // A bare macro name: undefined counts as 0; a defined one's value
            // is unknown to us, except that the platform macros above are 1.
            return Some(match defined(&token) {
                Truth::False => Truth::False,
                Truth::True => Truth::True,
                Truth::Unknown => Truth::Unknown,
            });
        }
        None // an operator we do not evaluate
    }
}

/// Tracks nested `#if` blocks while reading a file line by line.
#[derive(Default)]
pub struct Conditions {
    /// Per open `#if`: (this branch's truth, some earlier branch was certainly taken).
    stack: Vec<(Truth, bool)>,
}

impl Conditions {
    /// Feeds a directive (the text after `#`). Returns true if it was a
    /// conditional directive.
    pub fn directive(&mut self, directive: &str) -> bool {
        let (word, rest) = directive
            .split_once(|c: char| c.is_whitespace() || c == '(' || c == '!')
            .map_or((directive, ""), |(w, _)| (w, &directive[w.len()..]));
        match word {
            "if" => self.open(eval(rest)),
            "ifdef" => self.open(ifdef(rest, false)),
            "ifndef" => self.open(ifdef(rest, true)),
            "elif" | "elifdef" | "elifndef" => {
                let value = match word {
                    "elif" => eval(rest),
                    "elifdef" => ifdef(rest, false),
                    _ => ifdef(rest, true),
                };
                if let Some((current, taken)) = self.stack.last_mut() {
                    let taken_before = *taken || *current == Truth::True;
                    *current = if taken_before { Truth::False } else { value };
                    *taken = taken_before;
                }
            }
            "else" => {
                if let Some((current, taken)) = self.stack.last_mut() {
                    let taken_before = *taken || *current == Truth::True;
                    *current = if taken_before {
                        Truth::False
                    } else if *current == Truth::False {
                        Truth::True
                    } else {
                        Truth::Unknown
                    };
                    *taken = taken_before;
                }
            }
            "endif" => {
                self.stack.pop();
            }
            _ => return false,
        }
        true
    }

    fn open(&mut self, value: Truth) {
        self.stack.push((value, false));
    }

    /// False inside a block that is certainly not compiled.
    pub fn active(&self) -> bool {
        self.stack.iter().all(|(t, _)| *t != Truth::False)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_expressions() {
        assert_eq!(eval("0"), Truth::False);
        assert_eq!(eval("1"), Truth::True);
        assert_eq!(eval("defined(_WIN32)"), Truth::False);
        assert_eq!(eval("defined __linux__"), Truth::True);
        assert_eq!(eval("defined(_WIN32) || defined(__APPLE__)"), Truth::False);
        assert_eq!(
            eval("!defined(_MSC_VER) && defined(__clang__)"),
            Truth::True
        );
        assert_eq!(eval("defined(MY_FEATURE)"), Truth::Unknown);
        assert_eq!(eval("defined(_WIN32) && defined(MY_FEATURE)"), Truth::False);
        assert_eq!(eval("__cplusplus >= 202002L"), Truth::Unknown);
        assert_eq!(eval("_WIN32"), Truth::False);
    }

    #[test]
    fn tracks_branches() {
        let mut c = Conditions::default();
        c.directive("ifdef _WIN32");
        assert!(!c.active());
        c.directive("elif defined(__APPLE__)");
        assert!(!c.active());
        c.directive("else");
        assert!(c.active());
        c.directive("endif");
        assert!(c.active());

        c.directive("ifdef __linux__");
        assert!(c.active());
        c.directive("else");
        assert!(!c.active());
        c.directive("endif");

        c.directive("ifndef MY_HEADER_H"); // header guard: unknown, scanned
        assert!(c.active());
        c.directive("if 0");
        assert!(!c.active());
        c.directive("endif");
        c.directive("endif");
        assert!(c.active());
    }
}
