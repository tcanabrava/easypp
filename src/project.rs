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
use crate::cond::Conditions;
use crate::qt::{self, QtFiles, QtTools};
use crate::system::{OPENMP_PRAGMA, SysDep, System};
use crate::ui;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    C,
    Cpp,
    Cuda,
}

impl Lang {
    /// The language of a translation unit, or `None` if the file is not one.
    pub fn of(path: &Path) -> Option<Lang> {
        match path.extension()?.to_str()? {
            "c" => Some(Lang::C),
            "cu" => Some(Lang::Cuda),
            "cpp" | "cc" | "cxx" | "c++" | "cppm" | "ixx" | "ccm" | "cxxm" => Some(Lang::Cpp),
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
    /// Every internal library this one uses, directly or not.
    pub closure: Vec<String>,
    /// Files for moc, uic and rcc.
    pub qt: QtFiles,
    /// A folder in src/thirdparty with sources: compiled without -Werror.
    pub third_party: bool,
}

impl Library {
    fn new(name: String, dir: PathBuf, files: Vec<PathBuf>, third_party: bool) -> Self {
        Self {
            name,
            dir,
            sources: files
                .into_iter()
                .filter(|f| Lang::of(f).is_some())
                .collect(),
            deps: BTreeSet::new(),
            system: BTreeSet::new(),
            cflags: Vec::new(),
            closure: Vec::new(),
            qt: QtFiles::default(),
            third_party,
        }
    }

    /// Header-only libraries produce no archive.
    pub fn has_objects(&self) -> bool {
        !self.sources.is_empty()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
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
    /// Files for moc, uic and rcc.
    pub qt: QtFiles,
}

#[derive(Debug)]
pub struct Project {
    pub cuda: Option<crate::cuda::Toolkit>,
    pub root: PathBuf,
    pub libs: BTreeMap<String, Library>,
    /// Library names with dependencies before dependents (build order).
    pub lib_order: Vec<String>,
    pub bins: Vec<Target>,
    pub tests: Vec<Target>,
    /// Third-party include folders, passed with `-isystem`.
    pub system_includes: Vec<PathBuf>,
    /// moc/uic/rcc, when some component needs them.
    pub qt_tools: Option<QtTools>,
    pub modules: crate::modules::Modules,
}

impl Project {
    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        self.bins.iter().chain(&self.tests)
    }

    pub fn has_cuda_sources(&self) -> bool {
        crate::cuda::contains_sources(
            self.libs
                .values()
                .flat_map(|lib| &lib.sources)
                .chain(self.targets().flat_map(|target| &target.sources)),
        )
    }

    pub fn target_has_cuda_sources(&self, target: &Target) -> bool {
        crate::cuda::contains_sources(
            target
                .sources
                .iter()
                .chain(target.libs.iter().flat_map(|lib| &self.libs[lib].sources)),
        )
    }
}

pub const LIBS_DIR: &str = "src/libs";
pub(crate) const BIN_DIR: &str = "src/bin";
pub(crate) const TESTS_DIR: &str = "src/tests";
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
pub fn discover(
    root: &Path,
    opts: &crate::builder::Options,
    cache: Option<&mut Cache>,
) -> Result<Project, String> {
    discover_with_header_checks(root, opts, cache, None)
}

type HeaderCheck<'a> = &'a dyn Fn(&System, &str);

pub fn discover_with_header_checks(
    root: &Path,
    opts: &crate::builder::Options,
    mut cache: Option<&mut Cache>,
    check_header: Option<HeaderCheck<'_>>,
) -> Result<Project, String> {
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
        let files = walk(&entry);
        let lib = Library::new(file_name(&entry), entry, files, false);
        libs.insert(lib.name.clone(), lib);
    }

    // Third-party: each folder (or its include/ sub-folder) is a system include
    // path, and each folder is a library: header-only, or compiled from the
    // sources in it or its src/ (see thirdparty_files).
    let mut system_includes = Vec::new();
    let thirdparty = Path::new(THIRDPARTY_DIR);
    if thirdparty.is_dir() {
        system_includes.push(thirdparty.to_path_buf());
        for entry in sorted_entries(thirdparty) {
            if !entry.is_dir() {
                continue;
            }
            let include = entry.join("include");
            system_includes.push(if include.is_dir() {
                include
            } else {
                entry.clone()
            });

            let name = file_name(&entry);
            if libs.contains_key(&name) {
                return Err(format!(
                    "src/libs/{name} and src/thirdparty/{name} have the same name; rename one"
                ));
            }
            let files = thirdparty_files(&entry);
            libs.insert(name.clone(), Library::new(name, entry, files, true));
        }
    }

    let index = cache.as_mut().map(|c| c.take_index()).unwrap_or_default();
    let resolver = Resolver::new(&libs, &system_includes, index);
    let system = match cache.as_mut() {
        Some(c) => c.system(opts.cross.as_ref()),
        None => System::detect(opts.cross.as_ref()),
    };

    for lib in libs.values_mut().filter(|lib| lib.third_party) {
        // Vendored example programs are not part of the library.
        lib.sources.retain(|s| !resolver.defines_main(s));
    }
    let mut scan_flags = HashMap::new();
    let modules = crate::modules::Modules::discover(
        libs.values()
            .flat_map(|lib| {
                lib.sources
                    .iter()
                    .cloned()
                    .map(|src| (src, Some(lib.name.clone())))
            })
            .chain(
                [BIN_DIR, TESTS_DIR]
                    .into_iter()
                    .flat_map(|dir| walk(Path::new(dir)))
                    .map(|src| (src, None)),
            ),
        opts,
        |src, owner| {
            let component = owner
                .map(|name| libs[name].dir.as_path())
                .unwrap_or_else(|| {
                    let mut ancestors = src.ancestors();
                    ancestors
                        .find(|path| {
                            path.parent().is_some_and(|parent| {
                                parent == Path::new(BIN_DIR) || parent == Path::new(TESTS_DIR)
                            })
                        })
                        .unwrap_or(src)
                });
            let flags = scan_flags
                .entry(component.to_path_buf())
                .or_insert_with(|| {
                    // Resolve ordinary includes first so the scanner sees installed
                    // package headers and any defines those packages supply.
                    let empty = crate::modules::Modules::default();
                    let mut pending = vec![component.to_path_buf()];
                    let mut visited = HashSet::new();
                    let mut dependencies = BTreeSet::new();
                    while let Some(path) = pending.pop() {
                        if !visited.insert(path.clone()) {
                            continue;
                        }
                        let files = if path.is_file() {
                            vec![path]
                        } else {
                            walk(&path)
                        };
                        let scan = resolver.scan(files, None, &empty);
                        dependencies.extend(
                            scan.external
                                .iter()
                                .filter_map(|header| system.resolve(header)),
                        );
                        pending.extend(scan.libs.iter().map(|name| libs[name].dir.clone()));
                    }
                    system.compile_flags(&dependencies)
                });
            crate::builder::source_flags(opts, src, &system_includes, flags)
        },
    )?;
    for lib in libs.values_mut() {
        let files = if lib.third_party {
            thirdparty_files(&lib.dir)
        } else {
            walk(&lib.dir)
        };
        let scan = resolver.scan(files.clone(), Some(&lib.name), &modules);
        scan.check_headers(&system, check_header);
        lib.deps = scan.libs;
        lib.deps
            .extend(modules.libraries(&lib.sources, Some(&lib.name))?);
        lib.system = scan
            .external
            .iter()
            .filter_map(|i| system.resolve(i))
            .collect();
        if crate::cuda::contains_sources(&lib.sources) {
            lib.system.insert(SysDep::CudaRuntime);
        }
        lib.qt = QtFiles::from_files(&lib.dir, &files, scan.moc)?;
        lib.system.extend(framework_deps(&lib.qt, &system)?);
    }
    let lib_order = topo_order(&libs)?;

    let lib_cflags: Vec<(String, Vec<String>, Vec<String>)> = libs
        .values()
        .map(|lib| {
            let closure = link_order(&lib.deps, &libs, &lib_order);
            let mut used = lib.system.clone();
            for dep in &closure {
                used.extend(libs[dep].system.iter().cloned());
            }
            (lib.name.clone(), system.compile_flags(&used), closure)
        })
        .collect();
    for (name, cflags, closure) in lib_cflags {
        let lib = libs.get_mut(&name).unwrap();
        lib.cflags = cflags;
        lib.closure = closure;
    }

    let ctx = Context {
        resolver: &resolver,
        system: &system,
        libs: &libs,
        lib_order: &lib_order,
        modules: &modules,
        check_header,
    };
    let bins = discover_targets(Path::new(BIN_DIR), Kind::Bin, &ctx)?;
    let tests = discover_targets(Path::new(TESTS_DIR), Kind::Test, &ctx)?;

    if let Some(c) = cache
        && let Err(e) = c.save(&system, resolver.index.into_inner())
    {
        ui::warn(&format!("could not update the build cache: {e}"));
    }

    // Qt tools, only when something needs code generation.
    let needs_qt: Vec<(&str, &QtFiles)> = libs
        .values()
        .map(|l| (l.name.as_str(), &l.qt))
        .chain(bins.iter().chain(&tests).map(|t| (t.name.as_str(), &t.qt)))
        .filter(|(_, q)| q.needs_qt())
        .collect();
    if opts.cross.is_some() && !needs_qt.is_empty() {
        return Err(
            "Qt and KDE code generation are not available for the ARM64 QEMU target.".into(),
        );
    }
    for (_, files) in &needs_qt {
        for config in &files.kconfig {
            qt::kconfig_schema(config)?;
            if crate::kde::config_compiler().is_none() {
                return Err("KConfig generation needs kconfig_compiler_kf6 (install the KF6 Config development tools)".into());
            }
        }
    }
    let qt_tools = match needs_qt.first() {
        None => None,
        Some((name, _)) => {
            let deps = libs
                .values()
                .flat_map(|l| l.system.iter())
                .chain(bins.iter().chain(&tests).flat_map(|t| t.system.iter()));
            Some(qt::find_tools(deps).ok_or_else(|| {
                format!(
                    "'{name}' needs Qt code generation (moc, uic or rcc), but no Qt \
                     installation was found through its #includes and pkg-config"
                )
            })?)
        }
    };
    if needs_qt
        .iter()
        .any(|(_, files)| files.qml.as_ref().is_some_and(|qml| qml.module.is_some()))
        && qt_tools
            .as_ref()
            .is_none_or(|tools| tools.qmltyperegistrar.is_none())
    {
        return Err(
            "Named QML modules need qmltyperegistrar from Qt 6 declarative development tools"
                .into(),
        );
    }

    let cuda = (libs
        .values()
        .any(|lib| lib.system.contains(&SysDep::CudaRuntime))
        || bins
            .iter()
            .chain(&tests)
            .any(|target| target.system.contains(&SysDep::CudaRuntime)))
    .then(|| {
        if opts.cross.is_some() {
            return Err("CUDA builds are not available for the ARM64 QEMU target.".into());
        }
        crate::cuda::Toolkit::discover()
    })
    .transpose()?;
    if let Some(cuda) = &cuda
        && !system_includes.contains(&cuda.include)
    {
        system_includes.push(cuda.include.clone());
    }
    Ok(Project {
        cuda,
        root: root.to_path_buf(),
        libs,
        lib_order,
        bins,
        tests,
        system_includes,
        qt_tools,
        modules,
    })
}

struct Context<'a> {
    resolver: &'a Resolver,
    system: &'a System,
    libs: &'a BTreeMap<String, Library>,
    lib_order: &'a [String],
    modules: &'a crate::modules::Modules,
    check_header: Option<HeaderCheck<'a>>,
}

/// Dependencies implied by code-generation inputs, even without an include.
fn framework_deps(files: &QtFiles, system: &System) -> Result<Vec<SysDep>, String> {
    let mut deps = Vec::new();
    if !files.kconfig.is_empty() {
        deps.extend(system.resolve("KConfigSkeleton"));
    }
    if !files.gresource.is_empty() {
        if !crate::system::in_path("glib-compile-resources") {
            return Err(
                "GResource manifests need glib-compile-resources (install GLib development tools)"
                    .into(),
            );
        }
        deps.push(
            system
                .package("gio-2.0")
                .ok_or("GResource manifests need GIO development headers")?,
        );
    }
    if files.qml.is_some() {
        deps.push(
            system
                .package("Qt6Qml")
                .ok_or("QML resources need Qt 6 QML development files (Qt6Qml via pkg-config)")?,
        );
    }
    Ok(deps)
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

        let scan = ctx.resolver.scan(files.clone(), None, ctx.modules);
        scan.check_headers(ctx.system, ctx.check_header);
        let mut dependencies = scan.libs;
        dependencies.extend(ctx.modules.libraries(&sources, None)?);
        let libs = link_order(&dependencies, ctx.libs, ctx.lib_order);
        // A folder target owns its .ui/.qrc files; a single-file target has none.
        let qt = QtFiles::from_files(&entry, if entry.is_dir() { &files } else { &[] }, scan.moc)?;

        let mut system: Vec<SysDep> = Vec::new();
        let own = scan.external.iter().filter_map(|i| ctx.system.resolve(i));
        let from_libs = libs.iter().flat_map(|l| ctx.libs[l].system.iter().cloned());
        for dep in own.chain(from_libs) {
            // A GoogleTest/Catch2 test without its own main() gets the
            // framework's (gtest_main, catch2-with-main).
            let deps = if scan.defines_main {
                vec![dep]
            } else {
                ctx.system.with_main(&dep)
            };
            for dep in deps {
                if !system.contains(&dep) {
                    system.push(dep);
                }
            }
        }
        let cuda = crate::cuda::contains_sources(&sources).then_some(SysDep::CudaRuntime);
        for dep in framework_deps(&qt, ctx.system)?.into_iter().chain(cuda) {
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
            qt,
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
    /// file, then under src/libs (so `"mylib/header.hpp"`), then in
    /// src/thirdparty. Anything else is left to the system.
    fn resolve(&self, from: &Path, include: &str, search_here: bool) -> Option<PathBuf> {
        let here = search_here
            .then(|| from.parent())
            .flatten()
            .map(|p| p.join(include));
        let candidates = here
            .into_iter()
            .chain(std::iter::once(self.libs_root.join(include)))
            .chain(self.thirdparty.iter().map(|dir| dir.join(include)));
        for candidate in candidates {
            if candidate.is_file() {
                return fs::canonicalize(&candidate).ok();
            }
        }
        None
    }

    /// True if `file` defines `main()`.
    fn defines_main(&self, file: &Path) -> bool {
        let canonical = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
        self.file_includes(&canonical)
            .iter()
            .any(|i| i == DEFINES_MAIN)
    }

    fn file_includes(&self, canonical: &Path) -> Vec<String> {
        self.index.borrow_mut().includes(canonical, |text| {
            includes(text).into_iter().map(String::from).collect()
        })
    }

    /// The library that has `include` as a file, when it was written without
    /// the library's folder.
    fn library_with_file(&self, include: &str) -> Option<&str> {
        self.libs
            .iter()
            .find(|(_, dir, _)| dir.join(include).is_file())
            .map(|(name, _, _)| name.as_str())
    }

    /// A path relative to the project root, for messages.
    fn display(&self, canonical: &Path) -> String {
        let root = std::env::current_dir().unwrap_or_default();
        canonical
            .strip_prefix(&root)
            .unwrap_or(canonical)
            .display()
            .to_string()
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
        modules: &crate::modules::Modules,
    ) -> Scan {
        let mut found = BTreeSet::new();
        let mut external = BTreeSet::new();
        let mut visited: HashSet<PathBuf> = HashSet::new();
        let mut work: Vec<PathBuf> = files
            .iter()
            .map(|f| fs::canonicalize(f).unwrap_or_else(|_| f.clone()))
            .collect();
        visited.extend(work.iter().cloned());
        // The component's own files, to report which of them need moc.
        let own: HashMap<PathBuf, PathBuf> = work.iter().cloned().zip(files).collect();
        let mut moc = Vec::new();
        let mut defines_main = false;

        while let Some(file) = work.pop() {
            let file_includes = self.file_includes(&file);
            let headers = own
                .get(&file)
                .into_iter()
                .flat_map(|source| modules.headers(source))
                .map(|header| (&header.name, header.quoted));
            for (include, search_here) in
                file_includes.iter().map(|name| (name, true)).chain(headers)
            {
                if include == qt::MOC_MARKER {
                    if let Some(original) = own.get(&file) {
                        moc.push(original.clone());
                    }
                    continue;
                }
                if include == DEFINES_MAIN {
                    defines_main = defines_main || own.contains_key(&file);
                    continue;
                }
                if include == MACRO_INCLUDE {
                    // Only the project's own code: third-party headers may do
                    // this on purpose, and are not ours to fix.
                    let shown = self.display(&file);
                    if !shown.starts_with("src/thirdparty") {
                        ui::warn(&format!(
                            "{shown}: easypp cannot follow `#include` with a macro; if it \
                             names a library header, include that header directly"
                        ));
                    }
                    continue;
                }
                let Some(header) = self.resolve(&file, include, search_here) else {
                    if let Some(lib) = self.library_with_file(include) {
                        ui::warn(&format!(
                            "{} includes \"{include}\", which is in library '{lib}': \
                             write #include \"{lib}/{include}\"",
                            self.display(&file)
                        ));
                    }
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
        moc.sort();
        Scan {
            libs: found,
            external,
            moc,
            defines_main,
        }
    }
}

/// What scanning a component's files found.
struct Scan {
    /// Internal libraries it includes from.
    libs: BTreeSet<String>,
    /// Includes not found in the project, for the system to resolve.
    external: BTreeSet<String>,
    /// Its own files that need moc.
    moc: Vec<PathBuf>,
    /// One of its own files defines main().
    defines_main: bool,
}

impl Scan {
    fn check_headers(&self, system: &System, check: Option<HeaderCheck<'_>>) {
        if let Some(check) = check {
            for header in &self.external {
                check(system, header);
            }
        }
    }
}

/// What `includes()` reports for `#include SOME_MACRO`, which cannot be followed.
pub const MACRO_INCLUDE: &str = "#include macro";

/// What `includes()` reports for a file that defines `main()`.
pub const DEFINES_MAIN: &str = "#defines main";

/// `int main(`, `int main (void)` or GNU style `main (` on its own line.
fn defines_main(line: &str) -> bool {
    let line = line.trim_start();
    let line = line.strip_prefix("int").map_or(line, str::trim_start);
    line.strip_prefix("main")
        .is_some_and(|rest| rest.trim_start().starts_with('('))
}

/// The files of a third-party folder that belong to the library: those in the
/// folder itself and in its include/, src/ and source/ sub-folders. Examples,
/// tests, platform backends and docs in other sub-folders are left out.
fn thirdparty_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = sorted_entries(dir)
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    for sub in ["include", "src", "source"] {
        files.extend(walk(&dir.join(sub)));
    }
    files
}

/// Extracts the targets of `#include "..."` and `#include <...>` lines,
/// skipping blocks that are certainly off on this platform (`#ifdef _WIN32`,
/// `#if 0`; see [`crate::cond`]). Also reports, once per file each: a
/// `#pragma omp` line as [`OPENMP_PRAGMA`], a Q_OBJECT-style line as
/// [`qt::MOC_MARKER`], and `#include MACRO` as [`MACRO_INCLUDE`].
fn includes(text: &str) -> Vec<&str> {
    let mut conditions = Conditions::default();
    let (mut openmp, mut moc, mut macro_include, mut main) = (false, false, false, false);
    let mut out: Vec<&str> = Vec::new();
    for line in text.lines() {
        let directive = line.trim_start().strip_prefix('#').map(str::trim_start);
        if let Some(directive) = directive
            && conditions.directive(directive)
        {
            continue;
        }
        if !conditions.active() {
            continue;
        }
        let Some(directive) = directive else {
            moc = moc || qt::needs_moc(line);
            main = main || defines_main(line);
            continue;
        };
        if let Some(pragma) = directive.strip_prefix("pragma")
            && pragma.trim_start().starts_with("omp")
        {
            openmp = true;
            continue;
        }
        let Some(rest) = directive.strip_prefix("include") else {
            continue;
        };
        let rest = rest.trim_start();
        let close = match rest.chars().next() {
            Some('"') => '"',
            Some('<') => '>',
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                macro_include = true;
                continue;
            }
            _ => continue,
        };
        let rest = &rest[1..];
        if let Some(end) = rest.find(close) {
            out.push(&rest[..end]);
        }
    }
    if openmp {
        out.push(OPENMP_PRAGMA);
    }
    if moc {
        out.push(qt::MOC_MARKER);
    }
    if macro_include {
        out.push(MACRO_INCLUDE);
    }
    if main {
        out.push(DEFINES_MAIN);
    }
    out
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
        assert_eq!(includes(text), vec!["vector", "a/b.hpp", MACRO_INCLUDE]);
    }

    #[test]
    fn reports_openmp_pragmas() {
        let text =
            "#include <cstdio>\n  #pragma omp parallel for\n#pragma once\n# pragma  omp simd\n";
        assert_eq!(includes(text), vec!["cstdio", OPENMP_PRAGMA]);
        assert_eq!(includes("#pragma once\n"), Vec::<&str>::new());
    }

    #[test]
    fn skips_blocks_for_other_platforms() {
        let text = "#ifndef GUARD_H\n#define GUARD_H\n#ifdef _WIN32\n#include <windows.h>\n\
                    #elif defined(__APPLE__)\n#include <mach/mach.h>\n#else\n#include <unistd.h>\n\
                    #endif\n#if 0\n#include \"old.hpp\"\n#endif\n#if defined(HAVE_X)\n#include <x.h>\n\
                    #endif\n#include CONFIG_HEADER\n#endif\n";
        assert_eq!(includes(text), vec!["unistd.h", "x.h", MACRO_INCLUDE]);
    }

    #[test]
    fn reports_main() {
        assert!(includes("int main(int argc, char** argv)\n{").contains(&DEFINES_MAIN));
        assert!(includes("int main (void) {").contains(&DEFINES_MAIN));
        assert!(includes("static int\nmain (int argc)\n").contains(&DEFINES_MAIN));
        assert!(!includes("int mainloop(void);\n// int main()\n").contains(&DEFINES_MAIN));
    }

    #[test]
    fn reports_qt_meta_objects() {
        let text = "#include <QObject>\nclass A : public QObject {\n    Q_OBJECT\n};\n";
        assert_eq!(includes(text), vec!["QObject", qt::MOC_MARKER]);
    }

    #[test]
    fn language_by_extension() {
        assert_eq!(Lang::of(Path::new("a.c")), Some(Lang::C));
        assert_eq!(Lang::of(Path::new("a.cc")), Some(Lang::Cpp));
        assert_eq!(Lang::of(Path::new("a.hpp")), None);
    }
}
