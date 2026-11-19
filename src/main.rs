mod builder;
mod cache;
mod cond;
mod project;
mod qt;
mod scaffold;
mod system;
mod ui;
mod watch;

use builder::Options;
use project::{Project, Target};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Instant;

const USAGE: &str = "\
easypp: build small C and C++ projects with zero configuration

Usage: easypp <command> [options]

Commands:
  new <name>            Create a new project in ./<name>
  init                  Create the project layout in the current folder
  build                 Build all libraries and executables
  run [name] [-- args]  Build and run an executable from src/bin
  test [filter]         Build and run the programs in src/tests
  info                  Show what was discovered and what links to what
  watch                 Keep compile_commands.json (and Qt's generated files)
                        up to date as src/ changes (also: --watch)
  clean                 Remove the build/ folder

Options:
  -r, --release         Optimized build (-O2 -DNDEBUG) instead of debug (-O0 -g)
  -j, --jobs <N>        Parallel compile jobs (default: number of CPUs)
  -v, --verbose         Print every compiler command
      --no-cache        Ignore build/easypp.db (detect and scan everything)
  -h, --help            Show this help
  -V, --version         Show the version

Layout:
  src/libs/<name>/      static library <name>
  src/bin/<name>.cpp    executable <name> (or a folder src/bin/<name>/)
  src/tests/<name>.cpp  test <name>: passes when it exits with 0
  src/thirdparty/       third-party headers (-isystem)

#include \"<lib>/<header>\" and easypp links <lib> for you.
#include <SDL2/SDL.h> (or any system header) and easypp finds the system
library through pkg-config or /usr/lib, /usr/local/lib, ...";

struct Cli {
    command: String,
    positional: Vec<String>,
    passthrough: Vec<String>,
    opts: Options,
}

fn parse(args: Vec<String>) -> Result<Cli, String> {
    let mut command = None;
    let mut positional = Vec::new();
    let mut passthrough = Vec::new();
    let mut opts = Options {
        release: false,
        jobs: std::thread::available_parallelism().map_or(1, |n| n.get()),
        verbose: false,
        cache: true,
    };

    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--" => {
                passthrough.extend(it.by_ref());
                break;
            }
            "-r" | "--release" => opts.release = true,
            "--watch" if command.is_none() => command = Some("watch".to_string()),
            "-v" | "--verbose" => opts.verbose = true,
            "--no-cache" => opts.cache = false,
            "-h" | "--help" => command = Some("help".to_string()),
            "-V" | "--version" => command = Some("version".to_string()),
            "-j" | "--jobs" => {
                let value = it.next().ok_or("-j needs a number")?;
                opts.jobs = parse_jobs(&value)?;
            }
            s if s.starts_with("-j") && s.len() > 2 => opts.jobs = parse_jobs(&s[2..])?,
            s if s.starts_with('-') => {
                return Err(format!("unknown option '{s}' (see `easypp --help`)"));
            }
            _ if command.is_none() => command = Some(arg),
            _ => positional.push(arg),
        }
    }
    Ok(Cli {
        command: command.unwrap_or_else(|| "help".into()),
        positional,
        passthrough,
        opts,
    })
}

fn parse_jobs(value: &str) -> Result<usize, String> {
    match value.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("invalid job count '{value}'")),
    }
}

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
    let cli = parse(std::env::args().skip(1).collect())?;

    match cli.command.as_str() {
        "help" => {
            println!("{USAGE}");
            return Ok(ExitCode::SUCCESS);
        }
        "version" => {
            println!("easypp {}", env!("CARGO_PKG_VERSION"));
            return Ok(ExitCode::SUCCESS);
        }
        "new" => {
            let name = cli.positional.first().ok_or("usage: easypp new <name>")?;
            scaffold::new_project(Path::new(name))?;
            ui::status("Created", &format!("project '{name}'"));
            eprintln!("\n    cd {name} && easypp run\n");
            return Ok(ExitCode::SUCCESS);
        }
        "init" => {
            scaffold::init(Path::new("."))?;
            ui::status("Created", "project layout in the current folder");
            return Ok(ExitCode::SUCCESS);
        }
        "build" | "run" | "test" | "info" | "clean" | "watch" => {}
        other => return Err(format!("unknown command '{other}' (see `easypp --help`)")),
    }

    // Everything below works from the project root, with paths relative to it.
    let invoked_from = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = project::find_root(&invoked_from)?;
    std::env::set_current_dir(&root).map_err(|e| e.to_string())?;

    if cli.command == "clean" {
        if Path::new(builder::BUILD_DIR).exists() {
            std::fs::remove_dir_all(builder::BUILD_DIR)
                .map_err(|e| format!("cannot remove build/: {e}"))?;
        }
        ui::status("Removed", "build/");
        return Ok(ExitCode::SUCCESS);
    }

    if cli.command == "watch" {
        watch::run(&root, &cli.opts);
    }

    let project = load_project(&root, &cli.opts)?;
    match cli.command.as_str() {
        "build" => {
            let targets: Vec<&Target> = project.bins.iter().collect();
            builder::build(&project, &cli.opts, &targets)?;
            Ok(ExitCode::SUCCESS)
        }
        "run" => run_bin(&project, &cli, &invoked_from),
        "test" => run_tests(&project, &cli),
        "info" => {
            print_info(&project);
            Ok(ExitCode::SUCCESS)
        }
        _ => unreachable!(),
    }
}

/// Discovers the project at `root` (the current directory), using the cache
/// unless disabled. The cache is released before returning.
pub fn load_project(root: &Path, opts: &builder::Options) -> Result<project::Project, String> {
    let mut cache = if opts.cache {
        match cache::Cache::open(Path::new(builder::BUILD_DIR)) {
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
    let project = project::discover(root, cache.as_mut())?;
    if opts.verbose {
        match &cache {
            Some(c) if c.redetected() => {
                eprintln!("cache: detected the system, saved to build/easypp.db")
            }
            Some(_) => eprintln!("cache: system and include lists from build/easypp.db"),
            None => eprintln!("cache: not used"),
        }
    }
    Ok(project)
}

fn run_bin(project: &Project, cli: &Cli, invoked_from: &Path) -> Result<ExitCode, String> {
    let names = || {
        project
            .bins
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let target = match cli.positional.first() {
        Some(name) => project
            .bins
            .iter()
            .find(|b| &b.name == name)
            .ok_or_else(|| format!("no executable named '{name}'. Available: {}", names()))?,
        None => match project.bins.as_slice() {
            [only] => only,
            [] => return Err("there is nothing to run: src/bin has no .c/.cpp files".into()),
            _ => {
                return Err(format!(
                    "several executables exist; pick one: easypp run <{}>",
                    names()
                ));
            }
        },
    };

    builder::build(project, &cli.opts, &[target])?;
    let exe = project.root.join(builder::exe_path(&cli.opts, target));
    ui::status(
        "Running",
        &builder::exe_path(&cli.opts, target).display().to_string(),
    );
    let status = Command::new(&exe)
        .args(&cli.passthrough)
        .current_dir(invoked_from)
        .status()
        .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
    Ok(exit_code(status))
}

fn run_tests(project: &Project, cli: &Cli) -> Result<ExitCode, String> {
    let filter = cli.positional.first();
    let tests: Vec<&Target> = project
        .tests
        .iter()
        .filter(|t| filter.is_none_or(|f| t.name.contains(f.as_str())))
        .collect();
    if tests.is_empty() {
        eprintln!(
            "no tests found{}",
            filter.map_or(String::new(), |f| format!(" matching '{f}'"))
        );
        return Ok(ExitCode::SUCCESS);
    }

    builder::build(project, &cli.opts, &tests)?;
    eprintln!(
        "\nrunning {} test{}",
        tests.len(),
        if tests.len() == 1 { "" } else { "s" }
    );

    let mut failed = Vec::new();
    let start = Instant::now();
    for test in &tests {
        let exe = project.root.join(builder::exe_path(&cli.opts, test));
        let output = Command::new(&exe)
            .args(&cli.passthrough)
            .output()
            .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
        if output.status.success() {
            eprintln!("test {} ... {}", test.name, ui::green("ok"));
        } else {
            eprintln!("test {} ... {}", test.name, ui::red("FAILED"));
            failed.push((test.name.as_str(), output));
        }
    }

    for (name, output) in &failed {
        eprintln!("\n---- {name} ({}) ----", describe(output.status));
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
    let result = if failed.is_empty() {
        ui::green("ok")
    } else {
        ui::red("FAILED")
    };
    eprintln!(
        "\ntest result: {result}. {} passed; {} failed; finished in {:.2}s",
        tests.len() - failed.len(),
        failed.len(),
        start.elapsed().as_secs_f64()
    );
    Ok(if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn describe(status: std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("killed by signal {signal}");
        }
    }
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => "abnormal exit".into(),
    }
}

fn exit_code(status: std::process::ExitStatus) -> ExitCode {
    match status.code() {
        Some(code) => ExitCode::from(code.clamp(0, 255) as u8),
        None => ExitCode::FAILURE,
    }
}

fn print_info(project: &Project) {
    let list = |v: &[String]| {
        if v.is_empty() {
            "-".to_string()
        } else {
            v.join(", ")
        }
    };
    println!("project: {}\n", project.root.display());

    println!("libraries (build order):");
    if project.lib_order.is_empty() {
        println!("  (none)");
    }
    for name in &project.lib_order {
        let lib = &project.libs[name];
        let kind = if lib.has_objects() {
            format!("{} source file(s)", lib.sources.len())
        } else {
            "header-only".into()
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
