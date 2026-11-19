//! Compiling, archiving and linking, with incremental rebuilds.
//!
//! Every output (object, archive, executable) gets a `.cmd` stamp holding the
//! exact command that produced it. An output is rebuilt when it is missing,
//! when its command changed, or when any input is newer than it. For objects
//! the inputs come from the depfile clang writes (`-MMD`), so editing a
//! header rebuilds everything that includes it.

use crate::project::{Kind, Lang, Project, Target};
use crate::ui;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, SystemTime};

pub const BUILD_DIR: &str = "build";

const WARNINGS: [&str; 3] = ["-Wall", "-pedantic", "-Werror"];
const C_STD: &str = "-std=c17";
const CPP_STD: &str = "-std=c++20";

pub struct Options {
    pub release: bool,
    pub jobs: usize,
    pub verbose: bool,
    /// Use `build/easypp.db` to skip re-detection and re-scanning.
    pub cache: bool,
}

impl Options {
    pub fn profile(&self) -> &'static str {
        if self.release { "release" } else { "debug" }
    }

    fn dir(&self) -> PathBuf {
        Path::new(BUILD_DIR).join(self.profile())
    }
}

/// Where a target's executable ends up, relative to the project root.
pub fn exe_path(opts: &Options, target: &Target) -> PathBuf {
    let sub = match target.kind {
        Kind::Bin => "bin",
        Kind::Test => "tests",
    };
    opts.dir().join(sub).join(&target.name)
}

/// Builds every library plus the given executables.
pub fn build(project: &Project, opts: &Options, targets: &[&Target]) -> Result<(), String> {
    let start = Instant::now();
    write_compile_commands(project, opts)?;

    // 1. Compile (in parallel) everything that is out of date.
    let jobs: Vec<Job> = units(project, targets)
        .into_iter()
        .map(|(src, cflags)| {
            let obj = object_path(opts, src);
            let cmd = compile_command(project, opts, src, &obj, cflags);
            Job {
                src: src.clone(),
                obj,
                cmd,
            }
        })
        .filter(|job| {
            !depfile_inputs(&depfile_path(&job.obj))
                .is_some_and(|inputs| up_to_date(&job.obj, &job.cmd, &inputs))
        })
        .collect();
    compile_all(jobs, opts)?;

    // 2. One static archive per library that has sources.
    let ar = if crate::system::in_path("llvm-ar") {
        "llvm-ar"
    } else {
        "ar"
    };
    for name in &project.lib_order {
        let lib = &project.libs[name];
        if !lib.has_objects() {
            continue;
        }
        let archive = archive_path(opts, name);
        let objects: Vec<PathBuf> = lib.sources.iter().map(|s| object_path(opts, s)).collect();
        let mut cmd = vec![ar.to_string(), "rcs".into(), path_str(&archive)];
        cmd.extend(objects.iter().map(|o| path_str(o)));
        if !up_to_date(&archive, &cmd, &objects) {
            ui::status("Archiving", &format!("lib{name}.a"));
            let _ = fs::remove_file(&archive); // `ar r` would keep stale members.
            run_step(&archive, &cmd, opts.verbose)?;
        }
    }

    // 3. Link executables against the libraries they (transitively) include.
    for target in targets {
        let exe = exe_path(opts, target);
        let objects: Vec<PathBuf> = target
            .sources
            .iter()
            .map(|s| object_path(opts, s))
            .collect();
        let used: Vec<_> = target.libs.iter().map(|n| &project.libs[n]).collect();
        let archives: Vec<PathBuf> = used
            .iter()
            .filter(|l| l.has_objects())
            .map(|l| archive_path(opts, &l.name))
            .collect();

        let any_cpp = target
            .sources
            .iter()
            .any(|s| Lang::of(s) == Some(Lang::Cpp))
            || used.iter().any(|l| l.has_cpp());
        let mut cmd = vec![driver(if any_cpp { Lang::Cpp } else { Lang::C }).to_string()];
        cmd.extend(objects.iter().chain(&archives).map(|p| path_str(p)));
        cmd.extend(target.link_flags.iter().cloned());
        cmd.extend(["-o".into(), path_str(&exe)]);

        let inputs: Vec<PathBuf> = objects.into_iter().chain(archives).collect();
        if !up_to_date(&exe, &cmd, &inputs) {
            ui::status("Linking", &exe.display().to_string());
            run_step(&exe, &cmd, opts.verbose)?;
        }
    }

    let kind = if opts.release {
        "optimized"
    } else {
        "unoptimized + debuginfo"
    };
    ui::status(
        "Finished",
        &format!(
            "{} [{kind}] in {:.2}s",
            opts.profile(),
            start.elapsed().as_secs_f64()
        ),
    );
    Ok(())
}

struct Job {
    src: PathBuf,
    obj: PathBuf,
    cmd: Vec<String>,
}

fn compile_all(jobs: Vec<Job>, opts: &Options) -> Result<(), String> {
    if jobs.is_empty() {
        return Ok(());
    }
    let workers = opts.jobs.clamp(1, jobs.len());
    let queue = Mutex::new(jobs.into_iter());
    let failures = AtomicUsize::new(0);
    let spawn_error: Mutex<Option<String>> = Mutex::new(None);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    // Stop handing out work after the first failure, like make/cargo.
                    if failures.load(Ordering::Relaxed) > 0 {
                        break;
                    }
                    let Some(job) = queue.lock().unwrap().next() else {
                        break;
                    };

                    ui::status("Compiling", &job.src.display().to_string());
                    if opts.verbose {
                        eprintln!("{}", job.cmd.join(" "));
                    }
                    if let Some(parent) = job.obj.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = fs::remove_file(stamp_path(&job.obj));

                    let mut command = Command::new(&job.cmd[0]);
                    command.args(&job.cmd[1..]);
                    if ui::color() {
                        command.arg("-fcolor-diagnostics");
                    }
                    match command.output() {
                        Ok(out) => {
                            let mut err = std::io::stderr().lock();
                            let _ = err.write_all(&out.stdout);
                            let _ = err.write_all(&out.stderr);
                            if out.status.success() {
                                write_stamp(&job.obj, &job.cmd);
                            } else {
                                failures.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        Err(e) => {
                            failures.fetch_add(1, Ordering::Relaxed);
                            *spawn_error.lock().unwrap() = Some(spawn_failure(&job.cmd[0], &e));
                        }
                    }
                }
            });
        }
    });

    if let Some(e) = spawn_error.into_inner().unwrap() {
        return Err(e);
    }
    match failures.into_inner() {
        0 => Ok(()),
        1 => Err("a file failed to compile; fix the errors above".into()),
        n => Err(format!("{n} files failed to compile; fix the errors above")),
    }
}

/// Runs a single archive/link step and records its stamp on success.
fn run_step(output: &Path, cmd: &[String], verbose: bool) -> Result<(), String> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let _ = fs::remove_file(stamp_path(output));
    if verbose {
        eprintln!("{}", cmd.join(" "));
    }
    let status = Command::new(&cmd[0])
        .args(&cmd[1..])
        .status()
        .map_err(|e| spawn_failure(&cmd[0], &e))?;
    if !status.success() {
        return Err(format!(
            "`{}` failed while producing {}",
            cmd[0],
            output.display()
        ));
    }
    write_stamp(output, cmd);
    Ok(())
}

fn spawn_failure(program: &str, e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        format!("`{program}` was not found. Install clang/LLVM and make sure it is on your PATH.")
    } else {
        format!("failed to run `{program}`: {e}")
    }
}

fn driver(lang: Lang) -> &'static str {
    match lang {
        Lang::C => "clang",
        Lang::Cpp => "clang++",
    }
}

/// Every source of every library, plus those of `targets`, with the system
/// flags of the component it belongs to.
fn units<'a>(project: &'a Project, targets: &[&'a Target]) -> Vec<(&'a PathBuf, &'a [String])> {
    let libs = project.lib_order.iter().map(|n| &project.libs[n]);
    libs.flat_map(|l| l.sources.iter().map(|s| (s, l.cflags.as_slice())))
        .chain(
            targets
                .iter()
                .flat_map(|t| t.sources.iter().map(|s| (s, t.cflags.as_slice()))),
        )
        .collect()
}

fn compile_command(
    project: &Project,
    opts: &Options,
    src: &Path,
    obj: &Path,
    system_cflags: &[String],
) -> Vec<String> {
    let lang = Lang::of(src).expect("only sources are compiled");
    let mut cmd: Vec<String> = vec![
        driver(lang).into(),
        match lang {
            Lang::C => C_STD,
            Lang::Cpp => CPP_STD,
        }
        .into(),
    ];
    cmd.extend(WARNINGS.map(String::from));
    if opts.release {
        cmd.extend(["-O2".into(), "-DNDEBUG".into()]);
    } else {
        cmd.extend(["-O0".into(), "-g".into()]);
    }
    // `#include "mylib/header.hpp"` and `#include "header.hpp"` both work.
    cmd.push(format!("-I{}", crate::project::LIBS_DIR));
    cmd.extend(
        project
            .libs
            .values()
            .map(|l| format!("-I{}", l.dir.display())),
    );
    for dir in &project.system_includes {
        cmd.extend(["-isystem".into(), path_str(dir)]);
    }
    cmd.extend(system_cflags.iter().cloned());
    cmd.extend([
        "-MMD".into(),
        "-MP".into(),
        "-MF".into(),
        path_str(&depfile_path(obj)),
    ]);
    cmd.extend(["-c".into(), path_str(src), "-o".into(), path_str(obj)]);
    cmd
}

/// `build/compile_commands.json`, picked up by clangd, VS Code and CLion.
fn write_compile_commands(project: &Project, opts: &Options) -> Result<(), String> {
    let directory = json_str(&path_str(&project.root));
    let all: Vec<&Target> = project.bins.iter().chain(&project.tests).collect();

    let entries: Vec<String> = units(project, &all)
        .into_iter()
        .map(|(src, cflags)| {
            let obj = object_path(opts, src);
            let args: Vec<String> = compile_command(project, opts, src, &obj, cflags)
                .iter()
                .map(|a| json_str(a))
                .collect();
            format!(
                "  {{\n    \"directory\": {directory},\n    \"file\": {},\n    \"output\": {},\n    \"arguments\": [{}]\n  }}",
                json_str(&path_str(src)),
                json_str(&path_str(&obj)),
                args.join(", ")
            )
        })
        .collect();

    let path = Path::new(BUILD_DIR).join("compile_commands.json");
    let content = format!("[\n{}\n]\n", entries.join(",\n"));
    if fs::read_to_string(&path).ok().as_deref() == Some(content.as_str()) {
        return Ok(());
    }
    fs::create_dir_all(BUILD_DIR).map_err(|e| format!("cannot create {BUILD_DIR}/: {e}"))?;
    fs::write(&path, content).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `src/libs/foo/a.cpp` -> `build/debug/obj/libs/foo/a.cpp.o`
fn object_path(opts: &Options, src: &Path) -> PathBuf {
    let rel = src.strip_prefix("src").unwrap_or(src);
    with_suffix(&opts.dir().join("obj").join(rel), ".o")
}

fn archive_path(opts: &Options, lib: &str) -> PathBuf {
    opts.dir().join("lib").join(format!("lib{lib}.a"))
}

fn depfile_path(obj: &Path) -> PathBuf {
    with_suffix(obj, ".d")
}

fn stamp_path(output: &Path) -> PathBuf {
    with_suffix(output, ".cmd")
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn write_stamp(output: &Path, cmd: &[String]) {
    let _ = fs::write(stamp_path(output), cmd.join("\n"));
}

fn mtime(p: &Path) -> Option<SystemTime> {
    fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn up_to_date(output: &Path, cmd: &[String], inputs: &[PathBuf]) -> bool {
    let Some(built) = mtime(output) else {
        return false;
    };
    if fs::read_to_string(stamp_path(output)).ok().as_deref() != Some(cmd.join("\n").as_str()) {
        return false;
    }
    inputs.iter().all(|i| mtime(i).is_some_and(|m| m <= built))
}

/// Parses a Makefile-style depfile written by `clang -MMD -MP`.
fn depfile_inputs(path: &Path) -> Option<Vec<PathBuf>> {
    let text = fs::read_to_string(path)
        .ok()?
        .replace("\\\r\n", " ")
        .replace("\\\n", " ");
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    let flush = |current: &mut String, out: &mut Vec<PathBuf>| {
        // Tokens ending in ':' are rule targets, not inputs.
        if !current.is_empty() && !current.ends_with(':') {
            out.push(PathBuf::from(current.as_str()));
        }
        current.clear();
    };
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&' ') => {
                current.push(' ');
                chars.next();
            }
            c if c.is_whitespace() => flush(&mut current, &mut out),
            c => current.push(c),
        }
    }
    flush(&mut current, &mut out);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_depfiles() {
        let dir = std::env::temp_dir().join(format!("easypp-dep-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let dep = dir.join("a.d");
        fs::write(
            &dep,
            "obj/a.o: src/a.cpp \\\n  src/my\\ dir/b.hpp\nsrc/my\\ dir/b.hpp:\n",
        )
        .unwrap();
        let inputs = depfile_inputs(&dep).unwrap();
        assert_eq!(
            inputs,
            vec![
                PathBuf::from("src/a.cpp"),
                PathBuf::from("src/my dir/b.hpp")
            ]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn escapes_json() {
        assert_eq!(json_str("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}
