//! `easypp fmt` (clang-format) and `easypp lint` (clang-tidy), with defaults
//! chosen for students. A project's own `.clang-format` / `.clang-tidy`
//! always win; `easypp new` writes both files with these defaults, so the
//! editor (clangd) formats and lints exactly like the commands do.

use crate::builder::{self, Options};
use crate::project::{Lang, Project};
use crate::ui;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// WebKit-like: four-space indent, function braces on their own line, braces
/// around every body (no dangling-statement bugs), 120 columns.
pub const FORMAT_STYLE: &str = "BasedOnStyle: WebKit
NamespaceIndentation: None
Cpp11BracedListStyle: true
ColumnLimit: 120
AllowShortFunctionsOnASingleLine: Inline
InsertBraces: true
";

/// Checks that find real bugs and wasted work, without style nagging.
pub const TIDY_CHECKS: &str = "-*,\
bugprone-*,-bugprone-easily-swappable-parameters,-bugprone-narrowing-conversions,\
-bugprone-implicit-widening-of-multiplication-result,-bugprone-signed-bitwise,\
clang-analyzer-*,\
performance-*,-performance-enum-size,\
modernize-use-nullptr,modernize-use-override,modernize-loop-convert,\
modernize-make-unique,modernize-make-shared,modernize-use-emplace,\
readability-container-size-empty,readability-redundant-string-cstr,\
readability-misleading-indentation,\
misc-redundant-expression,misc-unused-using-decls,\
cppcoreguidelines-slicing,cppcoreguidelines-pro-type-member-init";

/// Only the project's own headers are checked, not third-party or system ones.
const TIDY_HEADERS: &str = "src/(libs|bin|tests)/";

/// The `.clang-tidy` file `easypp new` writes.
pub fn tidy_file() -> String {
    format!("Checks: '{TIDY_CHECKS}'\nHeaderFilterRegex: '{TIDY_HEADERS}'\n")
}

/// Keeps the warnings located in the project's own files (with their notes and
/// code excerpts). The static analyzer can follow a path from the project's
/// code into a system or third-party header and report it there; that is not
/// something the student can fix.
fn own_warnings(output: &str) -> (String, usize) {
    let is_header = |line: &str| line.contains(": warning: ") || line.contains(": error: ");
    let is_own = |line: &str| {
        ["src/libs/", "src/bin/", "src/tests/"]
            .iter()
            .any(|p| line.starts_with(p))
    };
    let mut kept = String::new();
    let mut count = 0;
    let mut keeping = false;
    for line in output.lines() {
        if is_header(line) {
            keeping = is_own(line);
            count += usize::from(keeping);
        }
        if keeping {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    (kept, count)
}

/// The project's own C/C++ files (not third-party, not generated).
fn own_files(root: &Path, sources_only: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = ["src/libs", "src/bin", "src/tests"]
        .iter()
        .map(|d| root.join(d))
        .collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let source = Lang::of(&path).is_some();
            let header = matches!(ext, "h" | "hh" | "hpp" | "hxx" | "inl" | "ipp" | "tpp");
            if source || (!sources_only && header) {
                out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
            }
        }
    }
    out.sort();
    out
}

fn tool(name: &str) -> Result<(), String> {
    if crate::system::in_path(name) {
        Ok(())
    } else {
        Err(format!(
            "`{name}` was not found; install clang-tools (it comes with LLVM)"
        ))
    }
}

/// `easypp fmt`: formats the project's files in place, or with `check` only
/// reports which ones are not formatted (for CI). Returns true if all is well.
pub fn format(root: &Path, check: bool) -> Result<bool, String> {
    tool("clang-format")?;
    let files = own_files(root, false);
    if files.is_empty() {
        return Ok(true);
    }
    let style = if root.join(".clang-format").exists() || root.join("_clang-format").exists() {
        "file".to_string()
    } else {
        format!("{{{}}}", FORMAT_STYLE.trim().replace('\n', ", "))
    };

    let before: Vec<Option<String>> = files
        .iter()
        .map(|f| fs::read_to_string(root.join(f)).ok())
        .collect();
    let mut command = Command::new("clang-format");
    command.current_dir(root).arg(format!("--style={style}"));
    if check {
        command.args(["--dry-run", "--Werror"]);
    } else {
        command.arg("-i");
    }
    let output = command
        .args(&files)
        .output()
        .map_err(|e| format!("cannot run clang-format: {e}"))?;

    if check {
        // clang-format prints every offending line; a list of files is enough.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut bad: Vec<&str> = stderr
            .lines()
            .filter_map(|l| l.split(':').next())
            .filter(|f| files.iter().any(|g| g.to_str() == Some(*f)))
            .collect();
        bad.dedup();
        for file in &bad {
            eprintln!("  not formatted: {file}");
        }
        if bad.is_empty() {
            ui::status("Formatted", &format!("all {} files already", files.len()));
        } else {
            ui::help("run `easypp fmt` to format them");
        }
        return Ok(bad.is_empty());
    }
    if !output.status.success() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return Err("clang-format failed".into());
    }
    let changed = files
        .iter()
        .zip(&before)
        .filter(|(f, old)| fs::read_to_string(root.join(f)).ok() != **old)
        .count();
    ui::status(
        "Formatted",
        &format!("{changed} of {} files changed", files.len()),
    );
    Ok(true)
}

/// `easypp lint`: runs clang-tidy on every source file in parallel. Returns
/// true when there are no warnings.
pub fn lint(project: &Project, opts: &Options) -> Result<bool, String> {
    tool("clang-tidy")?;
    // clang-tidy reads the compile commands, and Qt sources need their
    // generated headers.
    builder::update_compile_commands(project, opts)?;
    builder::generate(project, opts)?;

    let files = own_files(&project.root, true);
    let config = if project.root.join(".clang-tidy").exists() {
        None
    } else {
        Some(format!(
            "{{Checks: '{TIDY_CHECKS}', HeaderFilterRegex: '{TIDY_HEADERS}'}}"
        ))
    };

    let queue = Mutex::new(files.iter());
    let warnings = Mutex::new(0usize);
    let error: Mutex<Option<String>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..opts.jobs.clamp(1, files.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let Some(file) = queue.lock().unwrap().next() else {
                        break;
                    };
                    ui::status("Linting", &file.display().to_string());
                    let mut command = Command::new("clang-tidy");
                    command
                        .current_dir(&project.root)
                        .args(["-p", ".", "--quiet"]);
                    if let Some(config) = &config {
                        command.arg(format!("--config={config}"));
                    }
                    match command.arg(file).output() {
                        Ok(out) => {
                            let (text, count) = own_warnings(&String::from_utf8_lossy(&out.stdout));
                            if count > 0 {
                                eprint!("{text}");
                                *warnings.lock().unwrap() += count;
                            }
                        }
                        Err(e) => {
                            *error.lock().unwrap() = Some(format!("cannot run clang-tidy: {e}"))
                        }
                    }
                }
            });
        }
    });
    if let Some(e) = error.into_inner().unwrap() {
        return Err(e);
    }
    let warnings = warnings.into_inner().unwrap();
    if warnings == 0 {
        ui::status("Linted", &format!("{} files, no warnings", files.len()));
    } else {
        ui::warn(&format!(
            "{warnings} lint warning(s) in {} files",
            files.len()
        ));
    }
    Ok(warnings == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_warnings_in_project_files() {
        let output = "src/libs/a/a.cpp:3:5: warning: bad thing [check]\n    int x;\n    ^\n\
                      /usr/include/boost/x.hpp:9:1: warning: analyzer path [clang-analyzer-x]\n    y;\n\
                      src/libs/a/a.cpp:8:1: note: called from here\n\
                      src/thirdparty/z/z.c:1:1: warning: theirs [check]\n";
        let (text, count) = own_warnings(output);
        assert_eq!(count, 1);
        assert!(
            text.contains("bad thing")
                && !text.contains("analyzer path")
                && !text.contains("theirs")
        );
    }
}
