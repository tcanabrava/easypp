//! `easypp watch`: keeps `compile_commands.json` at the project root in step
//! with `src/`, so the editor's language server always knows every file and
//! its flags (new libraries, new files, new system libraries). For Qt
//! projects it also runs moc, uic and rcc, so generated headers such as
//! `ui_form.h` exist as soon as the `.ui` file is saved. It also builds the
//! compiled interfaces needed by C++ module imports.
//!
//! It polls: every half second it lists `src/` with modification times and
//! sizes, and when anything was added, removed or edited it rediscovers the
//! project and rewrites the file, but only if its content changed. Polling
//! needs no platform-specific API and is cheap for projects this size.

use crate::builder::{self, Options};
use crate::ui;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const INTERVAL: Duration = Duration::from_millis(500);

type Listing = Vec<(PathBuf, Option<SystemTime>, u64)>;

/// Runs until interrupted (Ctrl+C).
pub fn run(root: &Path, opts: &Options) -> ! {
    ui::status(
        "Watching",
        &format!("{}/src (Ctrl+C to stop)", root.display()),
    );
    let mut last: Option<Listing> = None;
    loop {
        let now = listing(Path::new("src"));
        if last.as_ref() != Some(&now) {
            regenerate(root, opts, last.is_none());
            last = Some(now);
        }
        std::thread::sleep(INTERVAL);
    }
}

fn regenerate(root: &Path, opts: &Options, first: bool) {
    // A broken state (say, a library cycle being edited) is reported, and the
    // watch goes on; the next save will try again.
    let project = match crate::load_project(root, opts) {
        Ok(project) => project,
        Err(e) => return ui::error(&e),
    };
    // Generated files go to build/, outside src/, so this cannot retrigger us.
    if let Err(e) = builder::generate(&project, opts) {
        ui::error(&e);
    }
    match builder::update_compile_commands(&project, opts) {
        Ok(Some(entries)) => ui::status(
            if first { "Wrote" } else { "Updated" },
            &format!("{} ({entries} files)", builder::COMPILE_DB),
        ),
        Ok(None) if first => ui::status("Up to date", builder::COMPILE_DB),
        Ok(None) => {}
        Err(e) => ui::error(&e),
    }
}

/// Every file under `dir` with its modification time and size, sorted.
fn listing(dir: &Path) -> Listing {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(path);
            } else {
                out.push((path, meta.modified().ok(), meta.len()));
            }
        }
    }
    out.sort();
    out
}
