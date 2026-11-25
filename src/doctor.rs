//! Read-only checks for the local toolchain and a project's build dependencies.
use crate::builder::{CPP_STD, Options};
use crate::{conan, project, system, ui};
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn run(start: &Path, opts: &Options) -> Result<bool, String> {
    let root = project::find_root(start).ok();
    match &root {
        Some(root) => ui::status("Checking", &root.display().to_string()),
        None => ui::status("Checking", "local toolchain"),
    }

    let clang = tool("clang", "Install Clang and put clang on PATH.");
    let clang_cpp = tool("clang++", "Install Clang and put clang++ on PATH.");
    let mut ok = clang && clang_cpp;
    if !opts.shared {
        if let Some(ar) = ["llvm-ar", "ar"]
            .into_iter()
            .find(|name| system::in_path(name))
        {
            ok &= tool(ar, "Install llvm-ar or the system binutils package.");
        } else {
            ui::error("Missing an archiver. Install llvm-ar or the system binutils package.");
            ok = false;
        }
    }
    if let Some(target) = &opts.cross {
        ui::status(
            "Target",
            &format!("{} ({})", crate::cross::TRIPLE, target.sysroot.display()),
        );
        match target.emulator() {
            Ok(emulator) => {
                ok &= tool(&emulator.to_string_lossy(), "Install QEMU user-mode tools.");
            }
            Err(e) => {
                ui::error(&e);
                ok = false;
            }
        }
    } else if let Some(debugger) = ["lldb", "gdb"]
        .into_iter()
        .find(|name| system::in_path(name))
    {
        ui::status("Found", debugger);
    } else {
        ui::warn("No debugger found. Install LLDB or GDB to use easypp debug.");
    }

    let target_flags: Vec<_> = opts
        .cross
        .iter()
        .flat_map(|target| target.link_flags())
        .collect();
    if clang_cpp {
        let mut flags = vec![CPP_STD];
        flags.extend(target_flags.iter().map(String::as_str));
        if opts.sanitizers() {
            flags.push("-fsanitize=address,undefined");
        }
        ok &= probe(
            "C++23 standard library and linker",
            &flags,
            "#include <iostream>\nint main() { std::cout << 0; }\n",
            if opts.cross.is_some() {
                "Install ARM64 C++ development files and a cross linker, and check the target sysroot."
            } else {
                "Install the C++ development files, linker, and Clang sanitizer runtimes for Debug builds."
            },
        );
    }

    if let Some(root) = root {
        std::env::set_current_dir(&root)
            .map_err(|e| format!("cannot enter {}: {e}", root.display()))?;
        if opts.cross.is_none() {
            conan::configure(&root, opts);
            if root.join("conanfile.txt").is_file() {
                ok &= check_conan(&root, opts);
            }
        } else if root.join("conanfile.txt").is_file() {
            ui::error(
                "This cross target does not use Conan dependencies. Install the ARM64 packages in its sysroot.",
            );
            ok = false;
        }

        let packages = RefCell::new(BTreeSet::new());
        let openmp = Cell::new(false);
        let check = |system: &system::System, header: &str| {
            if matches!(header, "omp.h" | system::OPENMP_PRAGMA) {
                openmp.set(true);
            }
            if let Some(package) = expected_package(header)
                && system.package(&package).is_none()
                && system.resolve(header).is_none()
            {
                packages.borrow_mut().insert(package);
            }
        };
        match project::discover_with_header_checks(&root, opts, None, Some(&check)) {
            Ok(project) => {
                ui::status(
                    "Found",
                    &format!(
                        "{} executable(s), {} test(s), {} libraries",
                        project.bins.len(),
                        project.tests.len(),
                        project.libs.len()
                    ),
                );
                if project.qt_tools.is_some() {
                    ui::status("Found", "Qt 6 code generation tools");
                }
                if let Some(cuda) = &project.cuda {
                    ok &= tool(
                        &cuda.nvcc.to_string_lossy(),
                        "Install a working NVIDIA CUDA Toolkit.",
                    );
                    ui::status("Found", &format!("CUDA Toolkit ({})", cuda.nvcc.display()));
                    if project.has_cuda_sources()
                        && let Err(e) = cuda.device_runtime()
                    {
                        ui::error(&e);
                        ok = false;
                    }
                }
            }
            Err(e) => {
                ui::error(&e);
                ok = false;
            }
        }
        for package in packages.into_inner() {
            ui::error(&format!(
                "Missing {package}. Install its development package so pkg-config can find it."
            ));
            ok = false;
        }
        if openmp.get() && clang_cpp {
            let mut flags = vec!["-fopenmp"];
            flags.extend(target_flags.iter().map(String::as_str));
            ok &= probe(
                "OpenMP headers and runtime",
                &flags,
                "#include <omp.h>\nint main() { return omp_get_max_threads() < 1; }\n",
                "Install the OpenMP development package for Clang (libomp).",
            );
        }
    }

    if ok {
        ui::status("Ready", "No required build tools are missing");
    }
    Ok(ok)
}

fn tool(name: &str, hint: &str) -> bool {
    match Command::new(name).arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            let detail = version.lines().next().unwrap_or("").trim();
            if detail.is_empty() {
                ui::status("Found", name);
            } else {
                ui::status("Found", &format!("{name} ({detail})"));
            }
            true
        }
        Ok(out) => {
            ui::error(&format!("{name} --version failed ({})", out.status));
            ui::help(hint);
            false
        }
        Err(e) => {
            ui::error(&format!("Cannot run {name}: {e}"));
            ui::help(hint);
            false
        }
    }
}

fn probe(label: &str, flags: &[&str], source: &str, hint: &str) -> bool {
    let result = (|| -> std::io::Result<_> {
        let mut child = Command::new("clang++")
            .args(flags)
            .args(["-x", "c++", "-", "-o", "/dev/null"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        let write = child.stdin.take().unwrap().write_all(source.as_bytes());
        let output = child.wait_with_output()?;
        if output.status.success() {
            write?;
        }
        Ok(output)
    })();
    match result {
        Ok(output) if output.status.success() => {
            ui::status("Found", label);
            return true;
        }
        Ok(output) => ui::error(&format!(
            "{label} check failed:\n{}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(e) => ui::error(&format!("{label} check failed: {e}")),
    }
    ui::help(hint);
    false
}

fn check_conan(root: &Path, opts: &Options) -> bool {
    let ready = conan::require_installed(root, opts);
    let mut ok = tool(
        "pkg-config",
        "Install pkg-config or pkgconf for Conan package discovery.",
    );
    if system::in_path("conan") {
        let version = Command::new("conan").arg("--version").output();
        match version {
            Ok(output)
                if output.status.success()
                    && String::from_utf8_lossy(&output.stdout).starts_with("Conan version 2.") =>
            {
                ui::status("Found", String::from_utf8_lossy(&output.stdout).trim())
            }
            _ => {
                ui::error("Conan on PATH is unusable. Install Conan 2.");
                ok = false;
            }
        }
    } else if let Some(installer) = ["pipx", "uv"]
        .into_iter()
        .find(|name| system::in_path(name))
    {
        ui::status(
            "Found",
            &format!("{installer} can set up Conan 2 on the first build"),
        );
    } else if ready.is_err() {
        ui::error("Conan 2 is missing. Install Conan 2, pipx, or uv.");
        ok = false;
    }
    if ready.is_ok() {
        ui::status("Found", "Conan dependency metadata");
    } else if ok {
        ui::warn(&format!(
            "Conan dependencies for {} need installation. easypp build will install them.",
            opts.profile()
        ));
    }
    ok
}

fn expected_package(header: &str) -> Option<String> {
    let package = match header {
        "gtk/gtk.h" => "gtk4",
        "gio/gio.h" => "gio-2.0",
        "SDL2/SDL.h" => "sdl2",
        "SDL3/SDL.h" => "sdl3",
        "QApplication" | "QWidget" => "Qt6Widgets",
        "QCoreApplication" | "QObject" => "Qt6Core",
        "QQmlApplicationEngine" | "QQmlEngine" => "Qt6Qml",
        _ => {
            let module = header.split('/').next()?;
            return matches!(
                module,
                "QtCore"
                    | "QtGui"
                    | "QtWidgets"
                    | "QtQml"
                    | "QtQuick"
                    | "QtQuickWidgets"
                    | "QtNetwork"
                    | "QtTest"
            )
            .then(|| format!("Qt6{}", &module[2..]));
        }
    };
    Some(package.into())
}
