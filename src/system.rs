//! System libraries (Linux only).
//!
//! An `#include` that is not part of the project is looked up in the
//! compiler's own search folders (`/usr/local/include`, `/usr/include`, ...),
//! and mapped to a library:
//!
//! 1. a pkg-config package whose name matches the header (`<SDL2/SDL.h>` ->
//!    `sdl2`, `<curl/curl.h>` -> `libcurl`), or whose private include folder
//!    contains it (`<SDL.h>` lives in `/usr/include/SDL2`, owned by `sdl2`);
//! 2. otherwise a `lib<name>.so` / `lib<name>.a` in `/usr/lib`, `/usr/lib64`,
//!    `/usr/local/lib`, ... (`<foo/foo.h>` or `<foo.h>` -> `-lfoo`).
//!
//! Flags for packages come from `pkg-config` (which follows `Requires:`), or
//! from the `.pc` file itself when `pkg-config` is not installed. System
//! libraries are linked with `--as-needed`, so a guess that turns out to be
//! unnecessary is dropped by the linker and costs nothing.

use crate::ui;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SysDep {
    /// A pkg-config package, by `.pc` file name.
    Pkg(String),
    /// A plain `-l<name>`. `dir` is set when the linker does not search it by default.
    Lib { name: String, dir: Option<PathBuf> },
    /// OpenMP: `-fopenmp` when compiling and linking. Found from `<omp.h>` or
    /// from any `#pragma omp` line, since pragmas need no header.
    OpenMp,
}

/// Bump whenever the header-to-library rules change, so cached answers from
/// older rules are thrown away (it is part of the fingerprint).
const RULES_VERSION: u32 = 9; // 2: lib<prefix>_<module>; 3: Qt modules; 4: no Qt 5; 6: latest version

/// What `includes()` reports for a `#pragma omp` line.
pub const OPENMP_PRAGMA: &str = "#pragma omp";

impl SysDep {
    pub fn label(&self) -> String {
        match self {
            SysDep::Pkg(name) if crate::conan::provides(name) => format!("{name} (Conan)"),
            SysDep::Pkg(name) => name.clone(),
            SysDep::OpenMp => "openmp".into(),
            SysDep::Lib { name, dir: None } => format!("-l{name}"),
            SysDep::Lib {
                name,
                dir: Some(dir),
            } => format!("-l{name} ({})", dir.display()),
        }
    }
}

/// Headers that live in libc but whose code is in a separate library.
fn libc_extra(include: &str) -> Option<&'static str> {
    match include {
        "math.h" | "complex.h" | "tgmath.h" | "fenv.h" => Some("m"),
        _ => None,
    }
}

const FALLBACK_INCLUDE_DIRS: &[&str] = &["/usr/local/include", "/usr/include"];
const EXTRA_LIB_DIRS: &[&str] = &[
    "/usr/local/lib",
    "/usr/local/lib64",
    "/usr/lib",
    "/usr/lib64",
    "/lib",
    "/lib64",
];
const FALLBACK_PC_DIRS: &[&str] = &[
    "/usr/local/lib/pkgconfig",
    "/usr/local/share/pkgconfig",
    "/usr/lib/pkgconfig",
    "/usr/lib64/pkgconfig",
    "/usr/share/pkgconfig",
];

/// Compile flags and link flags.
pub type Flags = (Vec<String>, Vec<String>);

#[derive(Clone, Debug, PartialEq)]
pub struct Package {
    pub name: String,
    /// `-I` folders from Cflags, minus the compiler's default ones.
    pub include_dirs: Vec<PathBuf>,
    pub cflags: Vec<String>,
    pub libs: Vec<String>,
}

/// Everything learned by asking clang and reading pkg-config files. This is
/// the slow part of start-up (tens of milliseconds), so it is cached per
/// project and only redone when [`Snapshot::fingerprint`] changes.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// The compiler's `#include <...>` search list.
    pub include_dirs: Vec<PathBuf>,
    /// The C++ standard library and compiler built-ins: never a library to link.
    pub builtin_dirs: Vec<PathBuf>,
    /// Folders searched for `lib<name>.so` / `.a`.
    pub lib_dirs: Vec<PathBuf>,
    /// Folders the linker already searches without `-L`.
    pub default_lib_dirs: Vec<PathBuf>,
    /// Folders searched for `.pc` files, existing or not.
    pub pc_dirs: Vec<PathBuf>,
    pub packages: Vec<Package>,
    pub pkg_config: bool,
}

pub struct System {
    snapshot: Snapshot,
    default_lib_dirs: HashSet<PathBuf>,
    by_name: HashMap<String, usize>,
    conan_packages: Vec<usize>,
    resolved: RefCell<HashMap<String, Option<SysDep>>>,
    pkg_flags: RefCell<HashMap<String, Flags>>,
}

impl Snapshot {
    pub fn detect() -> Snapshot {
        let mut include_dirs: Vec<PathBuf> = Vec::new();
        for lang in ["c++", "c"] {
            for dir in compiler_include_dirs(lang) {
                if !include_dirs.contains(&dir) {
                    include_dirs.push(dir);
                }
            }
        }
        if include_dirs.is_empty() {
            include_dirs = existing(FALLBACK_INCLUDE_DIRS.iter().map(PathBuf::from));
        }

        let resource_dir = capture("clang", &["-print-resource-dir"])
            .and_then(|out| fs::canonicalize(out.trim()).ok());
        let builtin_dirs = include_dirs
            .iter()
            .filter(|d| {
                d.components().any(|c| c.as_os_str() == "c++")
                    || resource_dir.as_ref().is_some_and(|r| d.starts_with(r))
            })
            .cloned()
            .collect();

        let default_lib_dirs: Vec<PathBuf> = capture("clang", &["-print-search-dirs"])
            .and_then(|out| {
                out.lines()
                    .find_map(|l| l.strip_prefix("libraries: =").map(String::from))
            })
            .map(|dirs| existing(dirs.split(':').map(PathBuf::from)))
            .unwrap_or_default();
        let mut lib_dirs = default_lib_dirs.clone();
        for dir in existing(EXTRA_LIB_DIRS.iter().map(PathBuf::from)) {
            if !lib_dirs.contains(&dir) {
                lib_dirs.push(dir);
            }
        }

        let pkg_config = in_path("pkg-config");
        let mut snapshot = Snapshot {
            include_dirs,
            builtin_dirs,
            lib_dirs,
            default_lib_dirs,
            pc_dirs: Vec::new(),
            packages: Vec::new(),
            pkg_config,
        };
        snapshot.load_packages();
        let tracked = crate::kde::packages(&snapshot.lib_dirs, &mut snapshot.packages);
        snapshot.pc_dirs.extend(tracked);
        snapshot
    }

    /// Cheap to compute: the tools' and folders' modification times. Installing
    /// or removing a library or package changes at least one of them.
    pub fn fingerprint(&self) -> String {
        let mut parts = vec![
            format!("easypp {} rules {RULES_VERSION}", env!("CARGO_PKG_VERSION")),
            format!("PKG_CONFIG_PATH={:?}", pkg_config_paths()),
        ];
        for program in ["clang", "clang++", "pkg-config"] {
            let path = find_in_path(program);
            parts.push(format!("{program} {:?} {}", path, stamp(path.as_deref())));
        }
        let dirs = self
            .include_dirs
            .iter()
            .chain(&self.lib_dirs)
            .chain(&self.pc_dirs);
        for dir in dirs {
            parts.push(format!("{} {}", dir.display(), stamp(Some(dir))));
        }
        for file in crate::conan::metadata_files() {
            parts.push(format!("{} {}", file.display(), stamp(Some(&file))));
        }
        parts.join("\n")
    }

    fn load_packages(&mut self) {
        let mut dirs = pkg_config_paths();
        match self
            .pkg_config
            .then(|| capture("pkg-config", &["--variable", "pc_path", "pkg-config"]))
            .flatten()
        {
            Some(path) => dirs.extend(path.trim().split(':').map(PathBuf::from)),
            None => dirs.extend(FALLBACK_PC_DIRS.iter().map(PathBuf::from)),
        }

        self.pc_dirs = dirs.clone();
        let mut seen = HashSet::new();
        for dir in dirs {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            let mut files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "pc"))
                .collect();
            files.sort();
            for file in files {
                let Some(mut package) = parse_pc(&file) else {
                    continue;
                };
                // Earlier folders win, like pkg-config itself.
                if !seen.insert(package.name.clone()) {
                    continue;
                }
                package
                    .include_dirs
                    .retain(|d| !self.include_dirs.contains(d));
                self.packages.push(package);
            }
        }
    }
}

impl System {
    /// Asks the compiler and pkg-config from scratch.
    pub fn detect() -> System {
        System::new(Snapshot::detect(), HashMap::new(), HashMap::new())
    }

    /// Builds the lookup state from a snapshot and previously resolved answers.
    pub fn new(
        snapshot: Snapshot,
        resolved: HashMap<String, Option<SysDep>>,
        pkg_flags: HashMap<String, Flags>,
    ) -> System {
        let mut by_name = HashMap::new();
        for (i, package) in snapshot.packages.iter().enumerate() {
            if !crate::qt::is_unsupported_package(&package.name) {
                by_name.entry(normalize(&package.name)).or_insert(i);
            }
        }
        let conan_packages = crate::conan::metadata_files()
            .iter()
            .filter_map(|path| path.file_stem()?.to_str())
            .filter_map(|name| by_name.get(&normalize(name)).copied())
            .collect();
        System {
            default_lib_dirs: snapshot.default_lib_dirs.iter().cloned().collect(),
            snapshot,
            by_name,
            conan_packages,
            resolved: RefCell::new(resolved),
            pkg_flags: RefCell::new(pkg_flags),
        }
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Every header and package answered so far, for the cache.
    pub fn answers(&self) -> (HashMap<String, Option<SysDep>>, HashMap<String, Flags>) {
        (
            self.resolved.borrow().clone(),
            self.pkg_flags.borrow().clone(),
        )
    }

    pub fn package(&self, name: &str) -> Option<SysDep> {
        self.by_name
            .get(&normalize(name))
            .map(|&i| SysDep::Pkg(self.snapshot.packages[i].name.clone()))
    }

    /// The system library behind an `#include`, if any.
    pub fn resolve(&self, include: &str) -> Option<SysDep> {
        if let Some(hit) = self.resolved.borrow().get(include) {
            return hit.clone();
        }
        let dep = self.lookup(include);
        self.resolved
            .borrow_mut()
            .insert(include.to_string(), dep.clone());
        dep
    }

    fn lookup(&self, include: &str) -> Option<SysDep> {
        // <omp.h> ships with the compiler, so it would otherwise count as a built-in.
        if include == "omp.h" || include == OPENMP_PRAGMA {
            return Some(SysDep::OpenMp);
        }
        if let Some(dep) = self.qt_module(include) {
            return Some(dep);
        }
        if let Some(name) = libc_extra(include) {
            return Some(SysDep::Lib {
                name: name.into(),
                dir: None,
            });
        }

        // Prefer a selected Conan package that actually ships this header.
        // This also covers headers that have a system copy under /usr/include.
        let mut conan_matches = self
            .conan_packages
            .iter()
            .map(|&i| &self.snapshot.packages[i])
            .filter(|p| p.include_dirs.iter().any(|dir| dir.join(include).is_file()));
        if let Some(first) = conan_matches.next() {
            // Components such as gtest and gmock can share an include directory.
            let names: Vec<_> = candidates(include)
                .iter()
                .map(|name| normalize(name))
                .collect();
            let rank = |p: &Package| names.iter().position(|name| *name == normalize(&p.name));
            let package = std::iter::once(first)
                .chain(conan_matches)
                .min_by_key(|p| rank(p).unwrap_or(usize::MAX))
                .unwrap();
            return Some(SysDep::Pkg(package.name.clone()));
        }

        // In the compiler's default folders: guess the library from the header's name.
        if let Some(dir) = self
            .snapshot
            .include_dirs
            .iter()
            .find(|d| d.join(include).is_file())
        {
            if self.snapshot.builtin_dirs.contains(dir) {
                return None;
            }
            return self
                .package_named_like(include)
                .or_else(|| self.library_file_named_like(include));
        }

        // Elsewhere: the package whose own include folder holds it. When several
        // do (gtk+-2.0, gtk+-3.0 and gtk4 all have <gtk/gtk.h>), always take the
        // latest version: prefer the most specific (deepest) folder, then the
        // highest version in the folder's name (gtk-4.0 over gtk-3.0, SDL2 over
        // SDL), then the package with the fewest folders, then the shortest
        // name (gtk4 over gtk4-x11, which share gtk-4.0).
        self.snapshot
            .packages
            .iter()
            .filter(|p| !crate::qt::is_unsupported_package(&p.name))
            .flat_map(|p| p.include_dirs.iter().map(move |d| (p, d)))
            .filter(|(_, d)| d.join(include).is_file())
            .min_by(|(pa, da), (pb, db)| {
                let depth = |d: &PathBuf| d.components().count();
                depth(db)
                    .cmp(&depth(da))
                    .then(folder_version(db).cmp(&folder_version(da)))
                    .then_with(|| gtk_variant(db).cmp(&gtk_variant(da)))
                    .then(pa.include_dirs.len().cmp(&pb.include_dirs.len()))
                    .then(pa.name.len().cmp(&pb.name.len()))
                    .then(pa.name.cmp(&pb.name))
            })
            .map(|(p, _)| SysDep::Pkg(p.name.clone()))
    }

    /// For a program without main(): a test framework's variant that provides
    /// one (gtest -> gtest_main, catch2 -> catch2-with-main); otherwise `dep`.
    pub fn with_main(&self, dep: &SysDep) -> Vec<SysDep> {
        let main_of = |name: &str| match name {
            "gtest" => Some("gtest_main"),
            "gmock" => Some("gmock_main"),
            "catch2" | "Catch2" => Some("catch2-with-main"),
            _ => None,
        };
        match dep {
            SysDep::Pkg(name) => match main_of(name).and_then(|m| {
                self.by_name
                    .get(&normalize(m))
                    .map(|&i| &self.snapshot.packages[i])
            }) {
                Some(package) => vec![SysDep::Pkg(package.name.clone())],
                None => vec![dep.clone()],
            },
            // No pkg-config file: libgtest_main.so next to libgtest.so.
            SysDep::Lib { name, dir } if matches!(name.as_str(), "gtest" | "gmock") => vec![
                SysDep::Lib {
                    name: format!("{name}_main"),
                    dir: dir.clone(),
                },
                dep.clone(),
            ],
            _ => vec![dep.clone()],
        }
    }

    /// `<QtWidgets/QPushButton>` or `<QtWidgets>` -> Qt6Widgets.
    /// Every Qt package lists the shared `include/qt6` folder, so the generic
    /// rules cannot tell the modules apart.
    fn qt_module(&self, include: &str) -> Option<SysDep> {
        let module = include.split('/').next()?;
        let rest = module.strip_prefix("Qt")?;
        if !rest.starts_with(|c: char| c.is_ascii_uppercase()) {
            return None;
        }
        let i = *self.by_name.get(&normalize(&format!("Qt6{rest}")))?;
        Some(SysDep::Pkg(self.snapshot.packages[i].name.clone()))
    }

    fn package_named_like(&self, include: &str) -> Option<SysDep> {
        candidates(include).into_iter().find_map(|c| {
            let n = normalize(&c);
            [n.clone(), format!("lib{n}")]
                .iter()
                .find_map(|key| self.by_name.get(key))
                .map(|&i| SysDep::Pkg(self.snapshot.packages[i].name.clone()))
        })
    }

    fn library_file_named_like(&self, include: &str) -> Option<SysDep> {
        // `<boost/json.hpp>` -> libboost_json: try the module library first.
        let names = module_library(include)
            .into_iter()
            .chain(candidates(include));
        for name in names {
            for dir in &self.snapshot.lib_dirs {
                let found = ["so", "a"]
                    .iter()
                    .any(|ext| dir.join(format!("lib{name}.{ext}")).is_file());
                if found {
                    let dir = (!self.default_lib_dirs.contains(dir)).then(|| dir.clone());
                    return Some(SysDep::Lib { name, dir });
                }
            }
        }
        None
    }

    /// Compiler flags for packages. `-I` becomes `-isystem`, so warnings inside
    /// system headers do not trip `-Werror`.
    pub fn compile_flags(&self, deps: &BTreeSet<SysDep>) -> Vec<String> {
        let mut out = Vec::new();
        for dep in deps {
            match dep {
                SysDep::Pkg(name) => {
                    for flag in self.flags_for(name).0 {
                        match flag.strip_prefix("-I") {
                            Some(dir) => push_pair(&mut out, "-isystem", dir),
                            None => push_unique(&mut out, flag),
                        }
                    }
                }
                SysDep::OpenMp => push_unique(&mut out, "-fopenmp".into()),
                SysDep::Lib { .. } => {}
            }
        }
        // Qt is built with QT_REDUCE_RELOCATIONS: code using it must be -fPIC.
        if deps.iter().any(crate::qt::is_qt) {
            push_unique(&mut out, "-fPIC".into());
        }
        out
    }

    /// Linker flags, in the order given (dependents before dependencies).
    pub fn link_flags(&self, deps: &[SysDep]) -> Vec<String> {
        if deps.is_empty() {
            return Vec::new();
        }
        let mut all = vec!["-Wl,--as-needed".to_string()];
        for dep in deps {
            match dep {
                SysDep::Pkg(name) => {
                    let flags = self.flags_for(name).1;
                    if crate::conan::provides(name) {
                        // RPATH also covers indirect shared libraries in Conan's cache.
                        all.extend(
                            flags
                                .iter()
                                .filter_map(|f| f.strip_prefix("-L"))
                                .filter(|d| Path::new(d).is_absolute())
                                .map(|d| format!("-Wl,--disable-new-dtags,-rpath,{d}")),
                        );
                    }
                    all.extend(flags);
                }
                SysDep::Lib { name, dir } => {
                    if let Some(dir) = dir {
                        all.push(format!("-L{}", dir.display()));
                        all.push(format!("-Wl,-rpath,{}", dir.display()));
                    }
                    all.push(format!("-l{name}"));
                }
                SysDep::OpenMp => all.push("-fopenmp".into()),
            }
        }
        dedup_link_flags(all)
    }

    fn flags_for(&self, name: &str) -> Flags {
        if let Some(flags) = self.pkg_flags.borrow().get(name) {
            return flags.clone();
        }
        let from_tool = (self.snapshot.pkg_config && !crate::kde::is_native(name)).then(|| {
            let run = |what: &str| -> Result<Vec<String>, String> {
                let out = pkg_config_command()
                    .args([what, name])
                    .stdin(Stdio::null())
                    .output()
                    .map_err(|e| e.to_string())?;
                if !out.status.success() {
                    return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
                }
                Ok(split_flags(&String::from_utf8_lossy(&out.stdout)))
            };
            Ok::<_, String>((run("--cflags")?, run("--libs")?))
        });
        let flags = match from_tool {
            Some(Ok(flags)) => flags,
            other => {
                if let Some(Err(e)) = other {
                    ui::warn(&format!(
                        "pkg-config failed for '{name}', using its .pc file as is: {e}"
                    ));
                }
                let p = self
                    .snapshot
                    .packages
                    .iter()
                    .find(|p| p.name == name)
                    .unwrap();
                (p.cflags.clone(), p.libs.clone())
            }
        };
        self.pkg_flags
            .borrow_mut()
            .insert(name.to_string(), flags.clone());
        flags
    }
}

/// Names a header might be named after: `SDL2/SDL.h` -> [SDL2, SDL].
fn candidates(include: &str) -> Vec<String> {
    let path = Path::new(include);
    let mut out = Vec::new();
    if include.contains('/')
        && let Some(first) = path.components().next()
    {
        out.push(first.as_os_str().to_string_lossy().into_owned());
    }
    if let Some(stem) = path.file_stem() {
        out.push(stem.to_string_lossy().into_owned());
    }
    out.dedup();
    out
}

/// `prefix_module` for `<prefix/module.hpp>` or `<prefix/module/...>`, the
/// naming used by Boost (`libboost_json`) and Abseil (`libabsl_strings`).
fn module_library(include: &str) -> Option<String> {
    let mut parts = Path::new(include).components();
    let prefix = parts.next()?.as_os_str().to_str()?;
    let second = Path::new(parts.next()?.as_os_str());
    let module = second.file_stem()?.to_str()?;
    Some(format!("{prefix}_{module}"))
}

/// The version in an include folder's name: `/usr/include/gtk-4.0` -> [4, 0],
/// `glibmm-2.68` -> [2, 68], `SDL2` -> [2], `SDL` -> []. (Package names are
/// not reliable for this: `gdk-x11-3.0` would read as version 11.)
// Some packages install parallel GTK variants, e.g. vte-2.91 and vte-2.91-gtk4.
fn gtk_variant(dir: &Path) -> u32 {
    dir.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.rsplit_once("-gtk"))
        .and_then(|(_, version)| version.parse().ok())
        .unwrap_or(0)
}

fn folder_version(dir: &Path) -> Vec<u32> {
    dir.file_name()
        .map_or_else(Vec::new, |name| version(&name.to_string_lossy()))
}

/// The first number in a name, as a version: `gtk-3.0` -> [3, 0].
fn version(name: &str) -> Vec<u32> {
    let Some(start) = name.find(|c: char| c.is_ascii_digit()) else {
        return Vec::new();
    };
    let rest = &name[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(rest.len());
    rest[..end]
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// `gtk+-3.0` -> `gtk30`, `SDL2` -> `sdl2`.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Removes repeated link flags without breaking static linking: a library
/// (`-lfoo`, `libfoo.a`, `libfoo.so`) keeps its *last* occurrence, so it
/// still comes after every library that needs it; other flags (`-L`,
/// `-Wl,...`) keep their first.
fn dedup_link_flags(flags: Vec<String>) -> Vec<String> {
    let is_library = |f: &str| {
        f.starts_with("-l") || f.ends_with(".a") || f.ends_with(".so") || f.contains(".so.")
    };
    let mut out: Vec<String> = Vec::new();
    for (i, flag) in flags.iter().enumerate() {
        let keep = if is_library(flag) {
            !flags[i + 1..].contains(flag)
        } else {
            !out.contains(flag)
        };
        if keep {
            out.push(flag.clone());
        }
    }
    out
}

fn push_unique(out: &mut Vec<String>, flag: String) {
    if !out.contains(&flag) {
        out.push(flag);
    }
}

fn push_pair(out: &mut Vec<String>, flag: &str, value: &str) {
    let exists = out.windows(2).any(|w| w[0] == flag && w[1] == value);
    if !exists {
        out.extend([flag.to_string(), value.to_string()]);
    }
}

pub(crate) fn split_flags(s: &str) -> Vec<String> {
    let mut flags = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if quote != Some('\'') => word.push(chars.next().unwrap_or(ch)),
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            ch if ch.is_whitespace() && quote.is_none() => {
                if !word.is_empty() {
                    flags.push(std::mem::take(&mut word));
                }
            }
            ch => word.push(ch),
        }
    }
    if !word.is_empty() {
        flags.push(word);
    }
    flags
}

fn existing(dirs: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in dirs {
        if let Ok(dir) = fs::canonicalize(&dir)
            && dir.is_dir()
            && !out.contains(&dir)
        {
            out.push(dir);
        }
    }
    out
}

/// The `#include <...>` search list clang uses for `lang` ("c" or "c++").
fn compiler_include_dirs(lang: &str) -> Vec<PathBuf> {
    let Ok(out) = Command::new("clang")
        .args(["-E", "-x", lang, "-", "-v"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    let stderr = String::from_utf8_lossy(&out.stderr);
    let dirs = stderr
        .lines()
        .skip_while(|l| !l.starts_with("#include <...> search starts here:"))
        .skip(1)
        .take_while(|l| !l.starts_with("End of search list."))
        .map(|l| PathBuf::from(l.trim()));
    existing(dirs)
}

fn pkg_config_paths() -> Vec<PathBuf> {
    crate::conan::package_dir()
        .into_iter()
        .map(Path::to_path_buf)
        .chain(
            std::env::var_os("PKG_CONFIG_PATH")
                .iter()
                .flat_map(std::env::split_paths),
        )
        .collect()
}

pub(crate) fn pkg_config_command() -> Command {
    let mut command = Command::new("pkg-config");
    if let Ok(paths) = std::env::join_paths(pkg_config_paths()) {
        command.env("PKG_CONFIG_PATH", paths);
    }
    command
}

fn capture(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn in_path(program: &str) -> bool {
    find_in_path(program).is_some()
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
        .map(|p| fs::canonicalize(&p).unwrap_or(p))
}

pub(crate) fn compiler_identity() -> String {
    let path = find_in_path("clang++");
    format!("{path:?} {}", stamp(path.as_deref()))
}

/// "mtime size" of a file or folder, or "missing".
fn stamp(path: Option<&Path>) -> String {
    let Some(meta) = path.and_then(|p| fs::metadata(p).ok()) else {
        return "missing".into();
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    format!("{mtime} {}", meta.len())
}

/// Minimal `.pc` reader: variables, `Cflags:` and `Libs:`.
fn parse_pc(path: &Path) -> Option<Package> {
    let text = fs::read_to_string(path).ok()?;
    let name = path.file_stem()?.to_string_lossy().into_owned();
    let mut vars: HashMap<String, String> = HashMap::new();
    if let Some(dir) = path.parent() {
        vars.insert("pcfiledir".into(), dir.to_string_lossy().into_owned());
    }
    let (mut cflags, mut libs) = (String::new(), String::new());

    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let colon = line.find(':');
        let equals = line.find('=');
        match (colon, equals) {
            (Some(c), e) if e.is_none_or(|e| c < e) => {
                let value = expand(line[c + 1..].trim(), &vars);
                match line[..c].trim() {
                    "Cflags" | "CFlags" => cflags = value,
                    "Libs" => libs = value,
                    _ => {}
                }
            }
            (_, Some(e)) => {
                let value = expand(line[e + 1..].trim(), &vars);
                vars.insert(line[..e].trim().to_string(), value);
            }
            _ => {}
        }
    }

    let cflags = split_flags(&cflags);
    let include_dirs = existing(
        cflags
            .iter()
            .filter_map(|f| f.strip_prefix("-I"))
            .map(PathBuf::from),
    );
    Some(Package {
        name,
        include_dirs,
        cflags,
        libs: split_flags(&libs),
    })
}

fn expand(value: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            break;
        };
        let key = &rest[start + 2..start + end];
        out.push_str(vars.get(key).map_or("", String::as_str));
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_name_candidates() {
        assert_eq!(candidates("SDL2/SDL.h"), vec!["SDL2", "SDL"]);
        assert_eq!(candidates("zlib.h"), vec!["zlib"]);
        assert_eq!(candidates("fmt/fmt.h"), vec!["fmt"]);
        assert_eq!(normalize("gtk+-3.0"), "gtk30");
        assert_eq!(
            module_library("boost/json.hpp").as_deref(),
            Some("boost_json")
        );
        assert_eq!(
            module_library("boost/program_options/variables_map.hpp").as_deref(),
            Some("boost_program_options")
        );
        assert_eq!(module_library("zlib.h"), None);
    }

    #[test]
    fn reads_versions_from_folder_names() {
        assert_eq!(
            folder_version(Path::new("/usr/include/gtk-3.0")),
            vec![3, 0]
        );
        assert_eq!(folder_version(Path::new("/usr/include/SDL2")), vec![2]);
        assert_eq!(
            folder_version(Path::new("/usr/include/SDL")),
            Vec::<u32>::new()
        );
        assert!(folder_version(Path::new("glibmm-2.68")) > folder_version(Path::new("glibmm-2.4")));
    }

    #[test]
    fn picks_the_latest_version_of_a_header() {
        // foo-4.0 has more include folders, which used to make foo-3.0 win.
        let root = std::env::temp_dir().join(format!("easypp-latest-{}", std::process::id()));
        for v in ["foo-3.0", "foo-4.0", "extra"] {
            fs::create_dir_all(root.join(v).join("foo")).unwrap();
            fs::write(root.join(v).join("foo/foo.h"), "").unwrap();
        }
        let package = |name: &str, dirs: &[&str]| Package {
            name: name.into(),
            include_dirs: dirs.iter().map(|d| root.join(d)).collect(),
            cflags: vec![],
            libs: vec![],
        };
        let snapshot = Snapshot {
            include_dirs: vec![],
            builtin_dirs: vec![],
            lib_dirs: vec![],
            default_lib_dirs: vec![],
            pc_dirs: vec![],
            packages: vec![
                package("foo-3.0", &["foo-3.0"]),
                package("foo-4.0", &["foo-4.0", "extra"]),
            ],
            pkg_config: false,
        };
        let system = System::new(snapshot, HashMap::new(), HashMap::new());
        assert_eq!(
            system.resolve("foo/foo.h"),
            Some(SysDep::Pkg("foo-4.0".into()))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn expands_pc_variables() {
        let mut vars = HashMap::new();
        vars.insert("prefix".to_string(), "/usr".to_string());
        vars.insert("includedir".to_string(), expand("${prefix}/include", &vars));
        assert_eq!(
            expand("-I${includedir}/SDL2 -D_REENTRANT", &vars),
            "-I/usr/include/SDL2 -D_REENTRANT"
        );
    }

    #[test]
    fn link_flags_are_deduplicated() {
        let snapshot = Snapshot {
            include_dirs: vec![],
            builtin_dirs: vec![],
            lib_dirs: vec![],
            default_lib_dirs: vec![],
            pc_dirs: vec![],
            packages: vec![],
            pkg_config: false,
        };
        let system = System::new(snapshot, HashMap::new(), HashMap::new());
        let m = SysDep::Lib {
            name: "m".into(),
            dir: None,
        };
        let local = SysDep::Lib {
            name: "foo".into(),
            dir: Some("/usr/local/lib".into()),
        };
        // Static order: a, z, b, z must become a, b, z (z after both users).
        let lib = |n: &str| SysDep::Lib {
            name: n.into(),
            dir: None,
        };
        assert_eq!(
            system.link_flags(&[lib("a"), lib("z"), lib("b"), lib("z")]),
            vec!["-Wl,--as-needed", "-la", "-lb", "-lz"]
        );
        assert_eq!(
            dedup_link_flags(
                [
                    "-L/x",
                    "-lfoo",
                    "/p/libbar.a",
                    "-L/x",
                    "-lbaz",
                    "/p/libbar.a"
                ]
                .map(String::from)
                .into()
            ),
            vec!["-L/x", "-lfoo", "-lbaz", "/p/libbar.a"]
        );
        assert_eq!(
            system.link_flags(&[local, m.clone(), m]),
            vec![
                "-Wl,--as-needed",
                "-L/usr/local/lib",
                "-Wl,-rpath,/usr/local/lib",
                "-lfoo",
                "-lm"
            ]
        );
    }
}
