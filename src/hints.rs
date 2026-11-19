//! Plain-language help for the compiler and linker errors beginners meet most.
//!
//! The compiler's message stays as it is; these are extra `help:` lines
//! computed from what easypp knows about the project.

use crate::project::Project;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Help for `fatal error: 'x' file not found`.
pub fn compile(project: &Project, output: &str) -> Vec<String> {
    let mut missing: Vec<String> = output
        .lines()
        .filter_map(|line| {
            let rest = &line[line.find("fatal error: '")? + "fatal error: '".len()..];
            let name = &rest[..rest.find('\'')?];
            line.contains("file not found").then(|| name.to_string())
        })
        .collect();
    missing.sort();
    missing.dedup();
    missing
        .iter()
        .map(|name| missing_header(project, name))
        .collect()
}

fn missing_header(project: &Project, name: &str) -> String {
    if let Some(lib) = project.libs.values().find(|l| l.dir.join(name).is_file()) {
        return format!(
            "'{name}' is in library '{}'; include it with its folder: #include \"{}/{name}\"",
            lib.name, lib.name
        );
    }
    if Path::new("/usr/include/qt").join(name).exists()
        || Path::new("/usr/include/qt")
            .join(qt_module_path(name))
            .exists()
    {
        return format!("'{name}' is a Qt 5 header; easypp supports Qt 6 only");
    }
    format!(
        "'{name}' is not part of the project. If it belongs to a library, install its \
         development package{}",
        package_search(name)
    )
}

/// `QDesktopWidget` -> `QtWidgets/QDesktopWidget` is unknown; only used to
/// also catch bare Qt class headers, which live in module folders.
fn qt_module_path(name: &str) -> PathBuf {
    let Ok(entries) = fs::read_dir("/usr/include/qt") else {
        return PathBuf::from(name);
    };
    entries
        .flatten()
        .map(|e| e.path().join(name))
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// How to find the package that ships a file, on this distribution.
fn package_search(name: &str) -> String {
    let os = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let ids: Vec<&str> = os
        .lines()
        .filter_map(|l| l.strip_prefix("ID=").or_else(|| l.strip_prefix("ID_LIKE=")))
        .flat_map(|v| v.trim_matches('"').split_whitespace())
        .collect();
    let file = name.rsplit('/').next().unwrap_or(name);
    let command = if ids.contains(&"arch") {
        format!("pacman -F {file}")
    } else if ids.iter().any(|id| *id == "debian" || *id == "ubuntu") {
        format!("apt-file search {name}")
    } else if ids.iter().any(|id| *id == "fedora" || *id == "rhel") {
        format!("dnf provides '*/{name}'")
    } else if ids.iter().any(|id| id.contains("suse")) {
        format!("zypper search --provides --file-list {file}")
    } else {
        return ".".into();
    };
    format!("; to find which one: {command}")
}

/// Help for `undefined reference to ...` (GNU ld) and `undefined symbol: ...` (lld).
pub fn link(project: &Project, output: &str) -> Vec<String> {
    let symbols: BTreeSet<String> = output
        .lines()
        .filter_map(|line| {
            if let Some(i) = line.find("undefined reference to `") {
                let rest = &line[i + "undefined reference to `".len()..];
                return rest.rfind('\'').map(|end| rest[..end].to_string());
            }
            line.find("undefined symbol: ")
                .map(|i| line[i + "undefined symbol: ".len()..].trim().to_string())
        })
        .collect();
    let sources = project_files(project);
    symbols
        .iter()
        .take(5)
        .map(|symbol| undefined(symbol, &sources))
        .collect()
}

fn undefined(symbol: &str, files: &[(PathBuf, String)]) -> String {
    if symbol == "main" {
        return "there is no main() function: every file in src/bin/ and src/tests/ \
                (or every folder there) is a program and needs one"
            .into();
    }
    if let Some(class) = symbol
        .strip_prefix("vtable for ")
        .or_else(|| symbol.strip_prefix("typeinfo for "))
    {
        return format!(
            "'{class}' has a virtual function that is declared but never defined; \
             define it (often the first one declared in the class) in a .cpp file"
        );
    }

    let name = base_name(symbol);

    // A member of a class declared in the project (in a header or a .cpp).
    if let Some(class) = class_name(symbol) {
        let declared: Vec<&Path> = files
            .iter()
            .filter(|(_, text)| declares_class(text, &class))
            .map(|(p, _)| p.as_path())
            .collect();
        if let Some(file) = declared.first() {
            let file = file.display();
            let defined = files
                .iter()
                .any(|(p, text)| !is_header(p) && text.contains(&format!("{class}::{name}")));
            return if name.starts_with('~') {
                format!(
                    "the destructor of '{class}' (declared in {file}) is never defined: \
                     write `{class}::{name}() {{ }}` in a .cpp file, or declare it \
                     `= default` in the class"
                )
            } else if defined {
                format!(
                    "`{symbol}` is needed, and a .cpp file defines {class}::{name}, but not \
                     with exactly these parameter types or qualifiers (const, ...); make the \
                     definition match the declaration in {file}"
                )
            } else {
                format!(
                    "`{symbol}` is declared in class '{class}' ({file}) but never defined: \
                     write `{class}::{name}(...) {{ ... }}` in a .cpp file"
                )
            };
        }
    }

    let declared: Vec<&Path> = files
        .iter()
        .filter(|(p, text)| is_header(p) && mentions(text, &name))
        .map(|(p, _)| p.as_path())
        .collect();
    let defined: Vec<&Path> = files
        .iter()
        .filter(|(p, text)| !is_header(p) && defines(text, &name))
        .map(|(p, _)| p.as_path())
        .collect();
    let list = |paths: &[&Path]| {
        paths
            .iter()
            .take(2)
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };

    match (declared.is_empty(), defined.is_empty()) {
        (false, true) => format!(
            "`{symbol}` is declared in {} but no .cpp file defines it: write its body \
             in a .cpp file of the same library",
            list(&declared)
        ),
        (_, false) => format!(
            "`{symbol}` is needed, and {} defines something called '{name}', but not with \
             exactly this name, namespace and parameter types; make the definition match \
             the declaration",
            list(&defined)
        ),
        (true, true) => format!(
            "`{symbol}` is not in this project, so it comes from a library: #include that \
             library's header (easypp links the libraries it sees included), and check \
             the spelling"
        ),
    }
}

/// `ns::Class::method(int, int) const` -> `method`; `operator<<` stays as is.
fn base_name(symbol: &str) -> String {
    let without_args = symbol.split('(').next().unwrap_or(symbol);
    if without_args.contains("operator") {
        return without_args
            .rsplit("::")
            .next()
            .unwrap_or(without_args)
            .trim()
            .to_string();
    }
    // Drop template arguments, which may themselves contain `::`.
    let mut plain = String::new();
    let mut depth = 0;
    for c in without_args.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            c if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    plain
        .rsplit("::")
        .next()
        .unwrap_or(&plain)
        .trim()
        .to_string()
}

/// `ns::Shape::area() const` -> `Shape`; `twice(int)` -> None.
fn class_name(symbol: &str) -> Option<String> {
    let without_args = symbol.split('(').next().unwrap_or(symbol);
    let mut parts: Vec<&str> = without_args.split("::").collect();
    parts.pop()?;
    let class = parts.pop()?.trim();
    let class = class.split('<').next().unwrap_or(class);
    (!class.is_empty()).then(|| class.to_string())
}

/// `class Shape` or `struct Shape` followed by a body or a base list.
fn declares_class(text: &str, class: &str) -> bool {
    ["class ", "struct "].iter().any(|keyword| {
        text.match_indices(&format!("{keyword}{class}"))
            .any(|(i, m)| {
                let after = text[i + m.len()..].trim_start();
                after.starts_with('{') || after.starts_with(':') || after.starts_with("final")
            })
    })
}

fn is_header(path: &Path) -> bool {
    crate::project::Lang::of(path).is_none()
}

fn mentions(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + name.len()..].trim_start().chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && after == Some('(')
    })
}

/// `name(...) {` somewhere: a definition rather than a call.
fn defines(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            return false;
        }
        let rest = &text[i + name.len()..];
        let Some(open) = rest.trim_start().strip_prefix('(') else {
            return false;
        };
        let Some(close) = open.find(')') else {
            return false;
        };
        let after = open[close + 1..].trim_start();
        after.starts_with('{')
            || after.starts_with("const")
            || after.starts_with("noexcept")
            || after.starts_with("override")
            || after.starts_with(':')
    })
}

/// The project's own files (not third-party), with their text.
fn project_files(project: &Project) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = ["src/libs", "src/bin", "src/tests"]
        .iter()
        .map(|d| project.root.join(d))
        .collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(text) = fs::read_to_string(&path) {
                let shown = path
                    .strip_prefix(&project.root)
                    .unwrap_or(&path)
                    .to_path_buf();
                out.push((shown, text));
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_base_names() {
        assert_eq!(base_name("blockfall::line_score(int, int)"), "line_score");
        assert_eq!(base_name("Foo::bar() const"), "bar");
        assert_eq!(base_name("std::vector<int>::size()"), "size");
        assert_eq!(base_name("twice"), "twice");
        assert_eq!(base_name("ns::Box<std::pair<int, int>>::get()"), "get");
    }

    #[test]
    fn tells_declarations_from_definitions() {
        let header = "int twice(int x);\n";
        let source = "int twice(long x)\n{\n    return 2 * x;\n}\n";
        let call = "int y = twice(3);\n";
        assert!(mentions(header, "twice"));
        assert!(!defines(header, "twice"));
        assert!(defines(source, "twice"));
        assert!(!defines(call, "twice"));
        assert!(!mentions("int thrice(int);", "twice"));
    }

    #[test]
    fn explains_undefined_symbols() {
        let files = vec![
            (
                PathBuf::from("src/libs/m/m.hpp"),
                "int twice(int);\nint half(int);\n".to_string(),
            ),
            (
                PathBuf::from("src/libs/m/m.cpp"),
                "int half(long x) { return x / 2; }\n".to_string(),
            ),
        ];
        assert!(undefined("twice(int)", &files).contains("no .cpp file defines it"));
        assert!(undefined("half(int)", &files).contains("not with exactly this name"));
        assert!(undefined("compress2", &files).contains("comes from a library"));
        assert!(undefined("main", &files).contains("no main() function"));
        assert!(undefined("vtable for Shape", &files).contains("virtual function"));

        let classes = vec![(
            PathBuf::from("src/bin/shapes.cpp"),
            "struct Shape {\n    virtual ~Shape();\n    double area() const;\n};\n".to_string(),
        )];
        assert!(undefined("Shape::~Shape()", &classes).contains("destructor of 'Shape'"));
        assert!(undefined("Shape::area() const", &classes).contains("declared in class 'Shape'"));
        assert_eq!(
            class_name("ns::Shape::area() const").as_deref(),
            Some("Shape")
        );
        assert_eq!(class_name("twice(int)"), None);
    }
}
