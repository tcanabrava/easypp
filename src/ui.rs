//! Terminal output helpers (cargo-style status lines).

use std::io::IsTerminal;
use std::sync::OnceLock;

pub fn color() -> bool {
    static COLOR: OnceLock<bool> = OnceLock::new();
    *COLOR.get_or_init(|| std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none())
}

fn paint(code: &str, text: &str) -> String {
    if color() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// Prints a right-aligned green verb followed by a message, e.g. `   Compiling src/foo.cpp`.
pub fn status(verb: &str, msg: &str) {
    eprintln!("{} {msg}", paint("1;32", &format!("{verb:>12}")));
}

pub fn warn(msg: &str) {
    eprintln!("{} {msg}", paint("1;33", "warning:"));
}

pub fn error(msg: &str) {
    eprintln!("{} {msg}", paint("1;31", "error:"));
}

pub fn red(text: &str) -> String {
    paint("1;31", text)
}

pub fn green(text: &str) -> String {
    paint("1;32", text)
}
