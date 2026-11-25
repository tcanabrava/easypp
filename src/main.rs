mod builder;
mod cache;
mod cli;
mod conan;
mod cond;
mod cross;
mod cuda;
mod doctor;
mod export;
mod hints;
mod install;
mod kde;
mod modules;
mod parallel;
mod project;
mod qt;
mod scaffold;
mod system;
mod testrun;
mod tools;
mod ui;
mod watch;

use builder::Options;
use clap::{CommandFactory, Parser};
use cli::{Actions, Cli, DependencyAction, ProgramArgs, TestArgs};
use project::{Project, Target};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Duration;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            ui::error(&e);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let Cli {
        command,
        build,
        watch,
    } = Cli::parse();
    if watch && command.is_some() {
        return Err(
            "--watch is an alias for the watch command and cannot be combined with another command"
                .into(),
        );
    }
    let action = match command {
        Some(command) => command,
        None if watch => Actions::Watch,
        None => {
            Cli::command().print_help().map_err(|e| e.to_string())?;
            return Ok(ExitCode::SUCCESS);
        }
    };
    let opts = Options::try_from(build)?;
    match &action {
        Actions::New { name } => {
            scaffold::new_project(Path::new(name))?;
            ui::status("Created", &format!("project '{name}'"));
            eprintln!("\n    cd {name} && easypp run\n");
            return Ok(ExitCode::SUCCESS);
        }
        Actions::Init => {
            scaffold::init(Path::new("."))?;
            ui::status("Created", "project layout in the current folder");
            return Ok(ExitCode::SUCCESS);
        }
        _ => {}
    }

    // Everything below works from the project root, with paths relative to it.
    let invoked_from = std::env::current_dir().map_err(|e| e.to_string())?;
    if matches!(action, Actions::Doctor) {
        return Ok(if doctor::run(&invoked_from, &opts)? {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    let root = project::find_root(&invoked_from)?;
    std::env::set_current_dir(&root).map_err(|e| e.to_string())?;

    if opts.cross.is_some() {
        match action {
            Actions::Debug(_) => return Err("Cross-target debugging needs QEMU's remote debugger. Use easypp run or test with this target.".into()),
            Actions::ExportCmake { .. } => return Err("CMake export for cross targets requires a target toolchain file. Build this target with easypp.".into()),
            Actions::Install => return Err("Build ARM64 programs with easypp build. Installing them into the host environment is unavailable.".into()),
            Actions::CudaArch { .. } | Actions::Deps { .. } => return Err("This command is unavailable for the ARM64 Linux target.".into()),
            _ => {}
        }
    }

    if opts.cross.is_none() {
        conan::configure(&root, &opts);
    }
    match action {
        Actions::Deps { action } => {
            let (references, args) = match action {
                DependencyAction::Add {
                    references,
                    install,
                } => (references, install.into_flags()),
                DependencyAction::Install(install) => (Vec::new(), install.into_flags()),
            };
            conan::install(&references, &args, &opts)?;
            return Ok(ExitCode::SUCCESS);
        }
        Actions::Clean => {
            let build = opts.cache_dir();
            if build.exists() {
                std::fs::remove_dir_all(&build)
                    .map_err(|e| format!("cannot remove {}: {e}", build.display()))?;
            }
            ui::status("Removed", &format!("{}/", build.display()));
            return Ok(ExitCode::SUCCESS);
        }
        Actions::Watch => watch::run(&root, &opts),
        Actions::Fmt { check } => {
            return Ok(if tools::format(&root, check)? {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            });
        }
        _ => {}
    }

    if opts.cross.is_none()
        && matches!(
            action,
            Actions::Build
                | Actions::Install
                | Actions::Run(_)
                | Actions::Debug(_)
                | Actions::Test(_)
        )
    {
        conan::ensure_installed(&root, &opts)?;
    }
    let project = load_project(&root, &opts)?;
    match action {
        Actions::Build => {
            let targets: Vec<&Target> = project.bins.iter().collect();
            builder::build(&project, &opts, &targets)?;
            Ok(ExitCode::SUCCESS)
        }
        Actions::Run(args) => run_bin(&project, &opts, &args, &invoked_from),
        Actions::Install => {
            install::run(&project, &opts)?;
            Ok(ExitCode::SUCCESS)
        }
        Actions::Debug(args) => debug_bin(&project, &opts, &args, &invoked_from),
        Actions::ExportCmake { force } => {
            export::write(&project, force, opts.shared)?;
            ui::status("Wrote", "CMakeLists.txt");
            eprintln!(
                "\n    cmake -S . -B build-cmake -G Ninja && cmake --build build-cmake && ctest --test-dir build-cmake\n"
            );
            Ok(ExitCode::SUCCESS)
        }
        Actions::Lint => Ok(if tools::lint(&project, &opts)? {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }),
        Actions::Test(args) => run_tests(&project, &opts, &args),
        Actions::CudaArch { architecture } => {
            let cuda = project
                .cuda
                .as_ref()
                .filter(|_| project.has_cuda_sources())
                .ok_or("this project has no .cu sources to configure")?;
            cuda.reconfigure(architecture.as_deref())?;
            Ok(ExitCode::SUCCESS)
        }
        Actions::Info => {
            print_info(&project, &opts);
            Ok(ExitCode::SUCCESS)
        }
        _ => unreachable!(),
    }
}

/// Discovers the project at `root` (the current directory), using the cache
/// unless disabled. The cache is released before returning.
pub fn load_project(root: &Path, opts: &builder::Options) -> Result<project::Project, String> {
    if opts.cross.is_none() {
        conan::require_installed(root, opts)?;
    } else if root.join("conanfile.txt").is_file() {
        return Err("This cross target does not use Conan dependencies. Install the ARM64 packages in its sysroot.".into());
    }
    let mut cache = if opts.cache {
        match cache::Cache::open(&opts.cache_dir()) {
            Ok(cache) => Some(cache),
            Err(e) => {
                ui::warn(&format!(
                    "build cache unavailable ({e}); continuing without it"
                ));
                None
            }
        }
    } else {
        None
    };
    let project = project::discover(root, opts, cache.as_mut())?;
    if let Some(cuda) = &project.cuda {
        if let Some(cache) = &cache {
            cuda.restore(cache)?;
        } else if !opts.cache && Path::new(builder::BUILD_DIR).join("easypp.db").is_file() {
            let database = cache::Cache::open(Path::new(builder::BUILD_DIR))?;
            cuda.restore(&database)?;
        }
    }
    if opts.verbose {
        match &cache {
            Some(c) if c.redetected() => {
                eprintln!(
                    "cache: detected the system, saved to {}",
                    opts.cache_dir().join("easypp.db").display()
                )
            }
            Some(_) => eprintln!(
                "cache: system and include lists from {}",
                opts.cache_dir().join("easypp.db").display()
            ),
            None => eprintln!("cache: not used"),
        }
    }
    Ok(project)
}

/// The program named on the command line (`debug` also accepts tests), or
/// the only program when there is just one.
fn pick_target<'a>(
    project: &'a Project,
    name: Option<&str>,
    with_tests: bool,
) -> Result<&'a Target, String> {
    let candidates: Vec<&Target> = if with_tests {
        project.targets().collect()
    } else {
        project.bins.iter().collect()
    };
    let names = || {
        candidates
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match name {
        Some(name) => candidates
            .iter()
            .find(|b| b.name == name)
            .copied()
            .ok_or_else(|| format!("no program named '{name}'. Available: {}", names())),
        None => match project.bins.as_slice() {
            [only] => Ok(only),
            [] => Err("there is nothing to run: src/bin has no .c/.cpp files".into()),
            _ => Err(format!(
                "several programs exist; pick one: easypp {} <{}>",
                if with_tests { "debug" } else { "run" },
                names()
            )),
        },
    }
}

/// Builds the program and starts it under lldb, LLVM's debugger, matching the
/// clang toolchain (gdb only when lldb is missing). Sanitizer errors abort, so
/// the debugger stops right where the bug is.
fn debug_bin(
    project: &Project,
    opts: &Options,
    args: &ProgramArgs,
    invoked_from: &Path,
) -> Result<ExitCode, String> {
    let target = pick_target(project, args.name.as_deref(), true)?;
    builder::build(project, opts, &[target])?;
    let exe = project.root.join(builder::exe_path(opts, target));

    let mut command = if system::in_path("lldb") {
        let mut c = Command::new("lldb");
        c.arg("--").arg(&exe);
        c
    } else if system::in_path("gdb") {
        let mut c = Command::new("gdb");
        c.args(["-q", "--args"]).arg(&exe);
        c
    } else {
        return Err("no debugger found: install lldb (it comes with LLVM)".into());
    };
    ui::status(
        "Debugging",
        &builder::exe_path(opts, target).display().to_string(),
    );
    runtime_data(&mut command, project);
    let status = command
        .args(&args.args)
        .current_dir(invoked_from)
        // LeakSanitizer cannot run under a debugger (ptrace); abort on errors
        // so the debugger stops at them.
        .env("ASAN_OPTIONS", "abort_on_error=1:detect_leaks=0")
        .env("UBSAN_OPTIONS", "abort_on_error=1:print_stacktrace=1")
        .status()
        .map_err(|e| format!("cannot start the debugger: {e}"))?;
    Ok(exit_code(status))
}

/// data/ mirrors a shared-data installation prefix (e.g. data/kmines/themes).
fn runtime_data(command: &mut Command, project: &Project) {
    let data = project.root.join("data");
    if data.is_dir() {
        let mut paths = vec![data];
        paths.extend(std::env::split_paths(
            &std::env::var_os("XDG_DATA_DIRS")
                .unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
        ));
        if let Ok(value) = std::env::join_paths(paths) {
            command.env("XDG_DATA_DIRS", value);
        }
    }
}

fn run_bin(
    project: &Project,
    opts: &Options,
    args: &ProgramArgs,
    invoked_from: &Path,
) -> Result<ExitCode, String> {
    let target = pick_target(project, args.name.as_deref(), false)?;
    builder::build(project, opts, &[target])?;
    let exe = project.root.join(builder::exe_path(opts, target));
    ui::status(
        "Running",
        &builder::exe_path(opts, target).display().to_string(),
    );
    let mut command = cross::command(opts.cross.as_ref(), &exe)?;
    runtime_data(&mut command, project);
    command.args(&args.args).current_dir(invoked_from);
    let qml = target.system.iter().any(|dep| {
        matches!(dep, system::SysDep::Pkg(name)
            if name.starts_with("Qt6Qml") || name.starts_with("Qt6Quick"))
    });
    let status = if qml {
        qt::run_qml(&mut command)
    } else {
        command.status()
    }
    .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
    Ok(exit_code(status))
}

fn run_tests(project: &Project, opts: &Options, args: &TestArgs) -> Result<ExitCode, String> {
    let filter = args.filter.as_deref();
    let tests: Vec<&Target> = project
        .tests
        .iter()
        .filter(|t| filter.is_none_or(|f| t.name.contains(f)))
        .collect();
    if tests.is_empty() {
        eprintln!(
            "no tests found{}",
            filter.map_or(String::new(), |f| format!(" matching '{f}'"))
        );
        return Ok(ExitCode::SUCCESS);
    }

    builder::build(project, opts, &tests)?;
    let tests: Vec<testrun::Test> = tests
        .iter()
        .map(|t| testrun::Test {
            name: t.name.clone(),
            exe: project.root.join(builder::exe_path(opts, t)),
        })
        .collect();
    let passed = testrun::run(
        &tests,
        &args.args,
        &project.root,
        opts.jobs,
        Duration::from_secs(args.timeout),
        opts.cross.as_ref(),
    )?;
    Ok(if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn exit_code(status: std::process::ExitStatus) -> ExitCode {
    match status.code() {
        Some(code) => ExitCode::from(code.clamp(0, 255) as u8),
        None => ExitCode::FAILURE,
    }
}

fn print_info(project: &Project, opts: &Options) {
    let list = |v: &[String]| {
        if v.is_empty() {
            "-".to_string()
        } else {
            v.join(", ")
        }
    };
    println!("project: {}\n", project.root.display());
    if let Some(target) = &opts.cross {
        println!(
            "target: {} (sysroot {})\n",
            cross::TRIPLE,
            target.sysroot.display()
        );
    }
    if let Some(cuda) = &project.cuda {
        println!(
            "CUDA architecture: {}\n",
            cuda.architecture().unwrap_or("choose on first CUDA build")
        );
    }

    println!("libraries (build order):");
    if project.lib_order.is_empty() {
        println!("  (none)");
    }
    for name in &project.lib_order {
        let lib = &project.libs[name];
        let kind = match (lib.has_objects(), lib.third_party) {
            (true, false) => format!("{} source file(s)", lib.sources.len()),
            (true, true) => format!("third-party, {} source(s)", lib.sources.len()),
            (false, false) => "header-only".into(),
            (false, true) => "third-party, headers".into(),
        };
        let deps: Vec<String> = lib.deps.iter().cloned().collect();
        let sys: Vec<String> = lib.system.iter().map(|d| d.label()).collect();
        println!("  {name:<20} {kind:<20} uses: {}", list(&deps));
        if !sys.is_empty() {
            println!("  {:<41} system: {}", "", sys.join(", "));
        }
        if !lib.qt.is_empty() {
            println!("  {:<41} qt: {}", "", lib.qt.summary());
        }
    }

    for (title, targets) in [("executables", &project.bins), ("tests", &project.tests)] {
        println!("\n{title}:");
        if targets.is_empty() {
            println!("  (none)");
        }
        for t in targets {
            println!("  {:<20} links: {}", t.name, list(&t.libs));
            if !t.system.is_empty() {
                let sys: Vec<String> = t.system.iter().map(|d| d.label()).collect();
                println!("  {:<20} system: {}", "", sys.join(", "));
            }
            if !t.qt.is_empty() {
                println!("  {:<20} qt: {}", "", t.qt.summary());
            }
        }
    }

    if !project.system_includes.is_empty() {
        let dirs: Vec<String> = project
            .system_includes
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        println!("\nthird-party include paths:\n  {}", dirs.join("\n  "));
    }
    if let Some(tools) = &project.qt_tools {
        println!(
            "\n{} tools:\n  {}\n  {}\n  {}",
            tools.version,
            tools.moc.display(),
            tools.uic.display(),
            tools.rcc.display()
        );
    }
}
