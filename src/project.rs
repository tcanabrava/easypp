//! Project discovery: finds libraries, executables and tests by looking at the
//! folder layout, and works out which internal and system libraries each one
//! needs by scanning `#include` directives.
//!
//! ```text
//! project_root/
//!     src/
//!         thirdparty/     header-only third-party code (added with -isystem)
//!         libs/<name>/    one static library per folder
//!         bin/            one executable per file (or per sub-folder)
//!         tests/          one test executable per file (or per sub-folder)
//! ```

use crate::cache::{Cache, IncludeIndex};
use crate::system::{SysDep, System};
use crate::ui;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    C,
    Cpp,
}

impl Lang {
    /// The language of a translation unit, or `None` if the file is not one.
    pub fn of(path: &Path) -> Option<Lang> {
        match path.extension()?.to_str()? {
            "c" => Some(Lang::C),
            "cpp" | "cc" | "cxx" | "c++" => Some(Lang::Cpp),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Library {
    pub name: String,
    /// Relative to the project root, e.g. `src/libs/greeting`.
    pub dir: PathBuf,
    pub sources: Vec<PathBuf>,
    /// Internal libraries this library includes headers from.
    pub deps: BTreeSet<String>,
    /// System libraries this library includes headers from.
    pub system: BTreeSet<SysDep>,
    /// Compiler flags for the system libraries used by this library and by
    /// the internal libraries it uses (their headers may include them).
    pub cflags: Vec<String>,
}

impl Library {
    /// Header-only libraries produce no archive.
    pub fn has_objects(&self) -> bool {
        !self.sources.is_empty()
    }

    pub fn has_cpp(&self) -> bool {
        self.sources.iter().any(|s| Lang::of(s) == Some(Lang::Cpp))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Bin,
    Test,
}

#[derive(Debug)]
pub struct Target {
    pub name: String,
    pub kind: Kind,
    pub sources: Vec<PathBuf>,
    /// Every internal library needed (transitively), in static-link order:
    /// a library always comes before the libraries it depends on.
    pub libs: Vec<String>,
    /// Every system library needed, its own first, then those of `libs`.
    pub system: Vec<SysDep>,
    /// Compiler and linker flags for `system`.
    pub cflags: Vec<String>,
    pub link_flags: Vec<String>,
}

#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub libs: BTreeMap<String, Library>,
    /// Library names with dependencies before dependents (build order).
    pub lib_order: Vec<String>,
    pub bins: Vec<Target>,
    pub tests: Vec<Target>,
    /// Third-party include folders, passed with `-isystem`.
    pub system_includes: Vec<PathBuf>,
}

pub const LIBS_DIR: &str = "src/libs";
const BIN_DIR: &str = "src/bin";
const TESTS_DIR: &str = "src/tests";
const THIRDPARTY_DIR: &str = "src/thirdparty";

/// Walks up from `start` to the first folder that looks like a project.
pub fn find_root(start: &Path) -> Result<PathBuf, String> {
    for dir in start.ancestors() {
        let src = dir.join("src");
        if ["libs", "bin", "tests"]
            .iter()
            .any(|d| src.join(d).is_dir())
        {
            return Ok(dir.to_path_buf());
        }
    }
    Err("no project found here or in any parent folder (expected src/bin, src/libs or src/tests).\n       \
         Create one with `easypp new <name>`."
        .into())
}

/// Discovers the project. The current directory must be the project root.
/// With a cache, unchanged files are not re-read and the system is not
/// re-detected.
pub fn discover(root: &Path, mut cache: Option<&mut Cache>) -> Result<Project, String> {
    // Libraries.
    let mut libs = BTreeMap::new();
    for entry in sorted_entries(Path::new(LIBS_DIR)) {
        if !entry.is_dir() {
            if Lang::of(&entry).is_some() {
                ui::warn(&format!(
                    "ignoring {}: library code must live in a folder, e.g. src/libs/<name>/",
                    entry.display()
                ));
            }
            continue;
        }
        let name = file_name(&entry);
        let sources = walk(&entry)
            .into_iter()
            .filter(|f| Lang::of(f).is_some())
            .collect();
        libs.insert(
            name.clone(),
            Library {
                name,
                dir: entry,
                sources,
                deps: BTreeSet::new(),
                system: BTreeSet::new(),
                cflags: Vec::new(),
            },
        );
    }

    // Third-party: each folder (or its include/ sub-folder) is a system include path.
    let mut system_includes = Vec::new();
    let thirdparty = Path::new(THIRDPARTY_DIR);
    if thirdparty.is_dir() {
        system_includes.push(thirdparty.to_path_buf());
        for entry in sorted_entries(thirdparty) {
            if entry.is_dir() {
                let include = entry.join("include");
                system_includes.push(if include.is_dir() { include } else { entry });
            }
        }
    }

    let index = cache.as_mut().map(|c| c.take_index()).unwrap_or_default();
    let resolver = Resolver::new(&libs, &system_includes, index);
    let system = match cache.as_mut() {
        Some(c) => c.system(),
        None => System::detect(),
    };

    for lib in libs.values_mut() {
        let (deps, external) = resolver.scan(walk(&lib.dir), Some(&lib.name));
        lib.deps = deps;
        lib.system = external.iter().filter_map(|i| system.resolve(i)).collect();
    }
    let lib_order = topo_order(&libs)?;

    let lib_cflags: Vec<(String, Vec<String>)> = libs
        .values()
        .map(|lib| {
            let mut used = lib.system.clone();
            for dep in link_order(&lib.deps, &libs, &lib_order) {
                used.extend(libs[&dep].system.iter().cloned());
            }
            (lib.name.clone(), system.compile_flags(&used))
        })
        .collect();
    for (name, cflags) in lib_cflags {
        libs.get_mut(&name).unwrap().cflags = cflags;
    }

    let ctx = Context {
        resolver: &resolver,
        system: &system,
        libs: &libs,
        lib_order: &lib_order,
    };
    let bins = discover_targets(Path::new(BIN_DIR), Kind::Bin, &ctx)?;
    let tests = discover_targets(Path::new(TESTS_DIR), Kind::Test, &ctx)?;

    if let Some(c) = cache
        && let Err(e) = c.save(&system, resolver.index.into_inner())
    {
        ui::warn(&format!("could not update the build cache: {e}"));
    }

    Ok(Project {
        root: root.to_path_buf(),
        libs,
        lib_order,
        bins,
        tests,
        system_includes,
    })
}

struct Context<'a> {
    resolver: &'a Resolver,
    system: &'a System,
    libs: &'a BTreeMap<String, Library>,
    lib_order: &'a [String],
}

fn discover_targets(dir: &Path, kind: Kind, ctx: &Context) -> Result<Vec<Target>, String> {
    let mut targets: Vec<Target> = Vec::new();
    let mut seen: HashMap<String, PathBuf> = HashMap::new();

    for entry in sorted_entries(dir) {
        let (name, files) = if entry.is_dir() {
            (file_name(&entry), walk(&entry))
        } else if Lang::of(&entry).is_some() {
            let stem = entry.file_stem().unwrap().to_string_lossy().into_owned();
            (stem, vec![entry.clone()])
        } else {
            continue;
        };

        let sources: Vec<PathBuf> = files
            .iter()
            .filter(|f| Lang::of(f).is_some())
            .cloned()
            .collect();
        if sources.is_empty() {
            ui::warn(&format!(
                "ignoring {}: it contains no .c/.cpp files",
                entry.display()
            ));
            continue;
        }
        if let Some(other) = seen.insert(name.clone(), entry.clone()) {
            return Err(format!(
                "{} and {} would both produce an executable named '{name}'",
                other.display(),
                entry.display()
            ));
        }

        let (direct, external) = ctx.resolver.scan(files, None);
        let libs = link_order(&direct, ctx.libs, ctx.lib_order);

        let mut system: Vec<SysDep> = Vec::new();
        let own = external.iter().filter_map(|i| ctx.system.resolve(i));
        let from_libs = libs.iter().flat_map(|l| ctx.libs[l].system.iter().cloned());
        for dep in own.chain(from_libs) {
            if !system.contains(&dep) {
                system.push(dep);
            }
        }
        let cflags = ctx.system.compile_flags(&system.iter().cloned().collect());
        let link_flags = ctx.system.link_flags(&system);

        targets.push(Target {
            name,
            kind,
            sources,
            libs,
            system,
            cflags,
            link_flags,
        });
    }
    Ok(targets)
}

/// All libraries reachable from `direct`, ordered dependents-first for the linker.
fn link_order(
    direct: &BTreeSet<String>,
    libs: &BTreeMap<String, Library>,
    lib_order: &[String],
) -> Vec<String> {
    let mut needed = HashSet::new();
    let mut stack: Vec<&String> = direct.iter().collect();
    while let Some(name) = stack.pop() {
        if needed.insert(name.clone()) {
            stack.extend(libs[name].deps.iter());
        }
    }
    lib_order
        .iter()
        .rev()
        .filter(|n| needed.contains(*n))
        .cloned()
        .collect()
}

/// Orders libraries so that dependencies come first; fails on cycles.
fn topo_order(libs: &BTreeMap<String, Library>) -> Result<Vec<String>, String> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Visiting,
        Done,
    }

    fn visit(
        name: &str,
        libs: &BTreeMap<String, Library>,
        state: &mut HashMap<String, State>,
        path: &mut Vec<String>,
        out: &mut Vec<String>,
    ) -> Result<(), String> {
        match state.get(name) {
            Some(State::Done) => return Ok(()),
            Some(State::Visiting) => {
                let start = path.iter().position(|p| p == name).unwrap();
                let mut cycle = path[start..].to_vec();
                cycle.push(name.to_string());
                return Err(format!(
                    "circular dependency between libraries: {}\n       \
                     Move the shared code into its own library that both can use.",
                    cycle.join(" -> ")
                ));
            }
            None => {}
        }
        state.insert(name.to_string(), State::Visiting);
        path.push(name.to_string());
        for dep in &libs[name].deps {
            visit(dep, libs, state, path, out)?;
        }
        path.pop();
        state.insert(name.to_string(), State::Done);
        out.push(name.to_string());
        Ok(())
    }

    let mut state = HashMap::new();
    let mut out = Vec::new();
    for name in libs.keys() {
        visit(name, libs, &mut state, &mut Vec::new(), &mut out)?;
    }
    Ok(out)
}

/// Maps `#include` directives to the internal library that provides the header.
struct Resolver {
    libs_root: PathBuf,
    /// (name, relative dir, canonical dir)
    libs: Vec<(String, PathBuf, PathBuf)>,
    thirdparty: Vec<PathBuf>,
    index: RefCell<IncludeIndex>,
}

impl Resolver {
    fn new(libs: &BTreeMap<String, Library>, thirdparty: &[PathBuf], index: IncludeIndex) -> Self {
        Resolver {
            libs_root: PathBuf::from(LIBS_DIR),
            libs: libs
                .values()
                .map(|l| {
                    let canonical = fs::canonicalize(&l.dir).unwrap_or_else(|_| l.dir.clone());
                    (l.name.clone(), l.dir.clone(), canonical)
                })
                .collect(),
            thirdparty: thirdparty.to_vec(),
            index: RefCell::new(index),
        }
    }

    /// Finds the header the same way the compiler will: next to the including
    /// file, then under src/libs, then inside each library folder, then in
    /// src/thirdparty. Anything else is left to the system.
    fn resolve(&self, from: &Path, include: &str) -> Option<PathBuf> {
        let here = from.parent().map(|p| p.join(include));
        let candidates = here
            .into_iter()
            .chain(std::iter::once(self.libs_root.join(include)))
            .chain(self.libs.iter().map(|(_, dir, _)| dir.join(include)))
            .chain(self.thirdparty.iter().map(|dir| dir.join(include)));
        for candidate in candidates {
            if candidate.is_file() {
                return fs::canonicalize(&candidate).ok();
            }
        }
        None
    }

    fn owner(&self, canonical: &Path) -> Option<&str> {
        self.libs
            .iter()
            .find(|(_, _, dir)| canonical.starts_with(dir))
            .map(|(name, _, _)| name.as_str())
    }

    /// Returns the internal libraries `files` include from, and the includes
    /// not found in the project (left for the system). Follows includes
    /// through non-library headers, e.g. one next to an executable, or a
    /// third-party header.
    fn scan(
        &self,
        files: Vec<PathBuf>,
        own_lib: Option<&str>,
    ) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut found = BTreeSet::new();
        let mut external = BTreeSet::new();
        let mut visited: HashSet<PathBuf> = HashSet::new();
        let mut work: Vec<PathBuf> = files
            .into_iter()
            .map(|f| fs::canonicalize(&f).unwrap_or(f))
            .collect();
        visited.extend(work.iter().cloned());

        while let Some(file) = work.pop() {
            let file_includes = self.index.borrow_mut().includes(&file, |text| {
                includes(text).into_iter().map(String::from).collect()
            });
            for include in &file_includes {
                let Some(header) = self.resolve(&file, include) else {
                    external.insert(include.to_string());
                    continue;
                };
                match self.owner(&header) {
                    Some(lib) if Some(lib) == own_lib => {}
                    Some(lib) => {
                        found.insert(lib.to_string());
                    }
                    None => {
                        if visited.insert(header.clone()) {
                            work.push(header);
                        }
                    }
                }
            }
        }
        (found, external)
    }
}

/// Extracts the targets of `#include "..."` and `#include <...>` lines.
fn includes(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| {
            let rest = line
                .trim_start()
                .strip_prefix('#')?
                .trim_start()
                .strip_prefix("include")?
                .trim_start();
            let close = match rest.chars().next()? {
                '"' => '"',
                '<' => '>',
                _ => return None,
            };
            let rest = &rest[1..];
            Some(&rest[..rest.find(close)?])
        })
        .collect()
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

/// Direct children of `dir`, sorted, skipping hidden entries. Empty if `dir` is missing.
fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<PathBuf> = read
        .filter_map(|e| e.ok())
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    entries.sort();
    entries
}

/// All files under `dir`, recursively, sorted.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in sorted_entries(dir) {
        if entry.is_dir() {
            out.extend(walk(&entry));
        } else {
            out.push(entry);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_include_lines() {
        let text = "#include <vector>\n  #  include \"a/b.hpp\" // x\n#define X\n#include MACRO\n";
        assert_eq!(includes(text), vec!["vector", "a/b.hpp"]);
    }

    #[test]
    fn language_by_extension() {
        assert_eq!(Lang::of(Path::new("a.c")), Some(Lang::C));
        assert_eq!(Lang::of(Path::new("a.cc")), Some(Lang::Cpp));
        assert_eq!(Lang::of(Path::new("a.hpp")), None);
    }
}
