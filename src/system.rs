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
}

impl SysDep {
    pub fn label(&self) -> String {
        match self {
            SysDep::Pkg(name) => name.clone(),
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
type Flags = (Vec<String>, Vec<String>);

struct Package {
    name: String,
    /// `-I` folders from Cflags, minus the compiler's default ones.
    include_dirs: Vec<PathBuf>,
    cflags: Vec<String>,
    libs: Vec<String>,
}

pub struct System {
    /// The compiler's `#include <...>` search list.
    include_dirs: Vec<PathBuf>,
    /// The C++ standard library and compiler built-ins: never a library to link.
    builtin_dirs: Vec<PathBuf>,
    /// Folders searched for `lib<name>.so` / `.a`.
    lib_dirs: Vec<PathBuf>,
    /// Folders the linker already searches without `-L`.
    default_lib_dirs: HashSet<PathBuf>,
    packages: Vec<Package>,
    by_name: HashMap<String, usize>,
    pkg_config: bool,
    resolved: RefCell<HashMap<String, Option<SysDep>>>,
    pkg_flags: RefCell<HashMap<String, Flags>>,
}

impl System {
    pub fn detect() -> System {
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
        let mut system = System {
            include_dirs,
            builtin_dirs,
            lib_dirs,
            default_lib_dirs: default_lib_dirs.into_iter().collect(),
            packages: Vec::new(),
            by_name: HashMap::new(),
            pkg_config,
            resolved: RefCell::new(HashMap::new()),
            pkg_flags: RefCell::new(HashMap::new()),
        };
        system.load_packages();
        system
    }

    fn load_packages(&mut self) {
        let mut dirs: Vec<PathBuf> = std::env::var_os("PKG_CONFIG_PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        match self
            .pkg_config
            .then(|| capture("pkg-config", &["--variable", "pc_path", "pkg-config"]))
            .flatten()
        {
            Some(path) => dirs.extend(path.trim().split(':').map(PathBuf::from)),
            None => dirs.extend(FALLBACK_PC_DIRS.iter().map(PathBuf::from)),
        }

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
                self.by_name
                    .entry(normalize(&package.name))
                    .or_insert(self.packages.len());
                self.packages.push(package);
            }
        }
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
        if let Some(name) = libc_extra(include) {
            return Some(SysDep::Lib {
                name: name.into(),
                dir: None,
            });
        }

        // In the compiler's default folders: guess the library from the header's name.
        if let Some(dir) = self.include_dirs.iter().find(|d| d.join(include).is_file()) {
            if self.builtin_dirs.contains(dir) {
                return None;
            }
            return self
                .package_named_like(include)
                .or_else(|| self.library_file_named_like(include));
        }

        // Elsewhere: the package whose own include folder holds it. When several
        // do (gtk+-2.0, gtk+-3.0, gtk4 all have <gtk/gtk.h>), prefer the most
        // specific folder, then the package with the fewest folders, then the
        // shortest name (gtk4 over gtk4-x11), then the newest-looking name.
        self.packages
            .iter()
            .flat_map(|p| p.include_dirs.iter().map(move |d| (p, d)))
            .filter(|(_, d)| d.join(include).is_file())
            .min_by(|(pa, da), (pb, db)| {
                let len = |d: &PathBuf| d.as_os_str().len();
                len(db)
                    .cmp(&len(da))
                    .then(pa.include_dirs.len().cmp(&pb.include_dirs.len()))
                    .then(pa.name.len().cmp(&pb.name.len()))
                    .then(pb.name.cmp(&pa.name))
            })
            .map(|(p, _)| SysDep::Pkg(p.name.clone()))
    }

    fn package_named_like(&self, include: &str) -> Option<SysDep> {
        candidates(include).into_iter().find_map(|c| {
            let n = normalize(&c);
            [n.clone(), format!("lib{n}")]
                .iter()
                .find_map(|key| self.by_name.get(key))
                .map(|&i| SysDep::Pkg(self.packages[i].name.clone()))
        })
    }

    fn library_file_named_like(&self, include: &str) -> Option<SysDep> {
        for name in candidates(include) {
            for dir in &self.lib_dirs {
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
            if let SysDep::Pkg(name) = dep {
                for flag in self.flags_for(name).0 {
                    match flag.strip_prefix("-I") {
                        Some(dir) => push_pair(&mut out, "-isystem", dir),
                        None => push_unique(&mut out, flag),
                    }
                }
            }
        }
        out
    }

    /// Linker flags, in the order given (dependents before dependencies).
    pub fn link_flags(&self, deps: &[SysDep]) -> Vec<String> {
        if deps.is_empty() {
            return Vec::new();
        }
        let mut out = vec!["-Wl,--as-needed".to_string()];
        for dep in deps {
            match dep {
                SysDep::Pkg(name) => {
                    for flag in self.flags_for(name).1 {
                        push_unique(&mut out, flag);
                    }
                }
                SysDep::Lib { name, dir } => {
                    if let Some(dir) = dir {
                        push_unique(&mut out, format!("-L{}", dir.display()));
                        push_unique(&mut out, format!("-Wl,-rpath,{}", dir.display()));
                    }
                    push_unique(&mut out, format!("-l{name}"));
                }
            }
        }
        out
    }

    fn flags_for(&self, name: &str) -> Flags {
        if let Some(flags) = self.pkg_flags.borrow().get(name) {
            return flags.clone();
        }
        let from_tool = self.pkg_config.then(|| {
            let run = |what: &str| -> Result<Vec<String>, String> {
                let out = Command::new("pkg-config")
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
                let p = self.packages.iter().find(|p| p.name == name).unwrap();
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

/// `gtk+-3.0` -> `gtk30`, `SDL2` -> `sdl2`.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
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

fn split_flags(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
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
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
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
        let system = System {
            include_dirs: vec![],
            builtin_dirs: vec![],
            lib_dirs: vec![],
            default_lib_dirs: HashSet::new(),
            packages: vec![],
            by_name: HashMap::new(),
            pkg_config: false,
            resolved: RefCell::new(HashMap::new()),
            pkg_flags: RefCell::new(HashMap::new()),
        };
        let m = SysDep::Lib {
            name: "m".into(),
            dir: None,
        };
        let local = SysDep::Lib {
            name: "foo".into(),
            dir: Some("/usr/local/lib".into()),
        };
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
