//! Runs test programs: in parallel, with a time limit each, output shown only
//! for the ones that fail.

use crate::ui;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Test {
    pub name: String,
    pub exe: PathBuf,
}

enum Outcome {
    Passed,
    Failed(ExitStatus),
    TimedOut,
}

struct Result {
    name: String,
    outcome: Outcome,
    output: Vec<u8>,
    took: Duration,
}

/// Runs every test from `root`, `jobs` at a time. Returns true if all passed.
pub fn run(
    tests: &[Test],
    args: &[String],
    root: &Path,
    jobs: usize,
    timeout: Duration,
    cross: Option<&crate::cross::Target>,
) -> std::result::Result<bool, String> {
    let s = if tests.len() == 1 { "" } else { "s" };
    eprintln!("\nrunning {} test{s}", tests.len());

    let start = Instant::now();
    let results: Mutex<Vec<Result>> = Mutex::new(Vec::new());
    let spawn_error: Mutex<Option<String>> = Mutex::new(None);

    crate::parallel::for_each(tests.iter(), jobs, |test| {
        match run_one(test, args, root, timeout, cross) {
            Ok(result) => {
                let label = match result.outcome {
                    Outcome::Passed => ui::green("ok"),
                    Outcome::Failed(_) => ui::red("FAILED"),
                    Outcome::TimedOut => ui::red("TIMEOUT"),
                };
                eprintln!(
                    "test {} ... {label} ({:.2}s)",
                    result.name,
                    result.took.as_secs_f64()
                );
                results.lock().unwrap().push(result);
            }
            Err(e) => *spawn_error.lock().unwrap() = Some(e),
        }
    });
    if let Some(e) = spawn_error.into_inner().unwrap() {
        return Err(e);
    }

    let mut results = results.into_inner().unwrap();
    results.sort_by(|a, b| a.name.cmp(&b.name));
    let failed: Vec<&Result> = results
        .iter()
        .filter(|r| !matches!(r.outcome, Outcome::Passed))
        .collect();
    for r in &failed {
        let why = match r.outcome {
            Outcome::Failed(status) => describe(status),
            Outcome::TimedOut => format!("killed after {}s; see --timeout", timeout.as_secs()),
            Outcome::Passed => unreachable!(),
        };
        eprintln!("\n---- {} ({why}) ----", r.name);
        eprint!("{}", String::from_utf8_lossy(&r.output));
    }

    let result = if failed.is_empty() {
        ui::green("ok")
    } else {
        ui::red("FAILED")
    };
    eprintln!(
        "\ntest result: {result}. {} passed; {} failed; finished in {:.2}s",
        results.len() - failed.len(),
        failed.len(),
        start.elapsed().as_secs_f64()
    );
    Ok(failed.is_empty())
}

fn run_one(
    test: &Test,
    args: &[String],
    root: &Path,
    timeout: Duration,
    cross: Option<&crate::cross::Target>,
) -> std::result::Result<Result, String> {
    let start = Instant::now();
    let mut child = crate::cross::command(cross, &test.exe)?
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", test.exe.display()))?;

    // Read both pipes on their own threads, so a chatty test cannot block on a
    // full pipe while we wait for it.
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = pipe.read_to_end(&mut buffer);
            buffer
        })
    })
    .collect();

    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break Outcome::Passed,
            Ok(Some(status)) => break Outcome::Failed(status),
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break Outcome::TimedOut;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(format!("cannot wait for {}: {e}", test.name)),
        }
    };
    let mut output = Vec::new();
    for reader in readers {
        output.extend(reader.join().unwrap_or_default());
    }
    Ok(Result {
        name: test.name.clone(),
        outcome,
        output,
        took: start.elapsed(),
    })
}

pub fn describe(status: ExitStatus) -> String {
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
