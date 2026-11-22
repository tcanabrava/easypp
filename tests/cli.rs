//! End-to-end tests: each one creates a project in a temporary folder and
//! drives the real `easypp` binary. They need clang (with its sanitizer
//! runtimes); `export_cmake_builds` also needs cmake and ninja.
//!
//! `cargo test` also builds and tests every example.
//! See the CI workflow for the additional dependencies.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Project {
    dir: PathBuf,
}

impl Project {
    fn temporary(test: &str) -> Project {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let parent = std::env::temp_dir().join(format!(
            "easypp-it-{}-{}-{}",
            std::process::id(),
            test,
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&parent).unwrap();
        Project {
            dir: parent.join("p"),
        }
    }

    /// `easypp new` in a fresh temporary folder.
    fn new(test: &str) -> Project {
        let project = Self::temporary(test);
        let out = Command::new(easypp())
            .arg("new")
            .arg("p")
            .current_dir(project.dir.parent().unwrap())
            .output()
            .unwrap();
        assert!(out.status.success(), "easypp new failed: {}", text(&out));
        project
    }

    fn copy_example(example: &Path) -> Project {
        let project = Self::temporary(&example.file_name().unwrap().to_string_lossy());
        fs::create_dir(&project.dir).unwrap();
        let mut copy = Command::new("cp");
        copy.arg("-a");
        for entry in fs::read_dir(example).unwrap() {
            let entry = entry.unwrap();
            if !matches!(
                entry.file_name().to_str(),
                Some("build" | "compile_commands.json" | "build-cmake")
            ) {
                copy.arg(entry.path());
            }
        }
        let out = copy.arg(&project.dir).output().unwrap();
        assert!(
            out.status.success(),
            "copy {}: {}",
            example.display(),
            text(&out)
        );
        project
    }

    fn write(&self, path: &str, content: &str) -> &Self {
        let path = self.dir.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
        self
    }

    fn remove(&self, path: &str) {
        let path = self.dir.join(path);
        if path.is_dir() {
            fs::remove_dir_all(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(easypp())
            .args(args)
            .current_dir(&self.dir)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "easypp {args:?} failed:\n{}",
            text(&out)
        );
        text(&out)
    }

    fn fails(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            !out.status.success(),
            "easypp {args:?} should have failed:\n{}",
            text(&out)
        );
        text(&out)
    }

    fn exists(&self, path: &str) -> bool {
        self.dir.join(path).exists()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.dir.parent().unwrap());
    }
}

fn easypp() -> &'static str {
    env!("CARGO_BIN_EXE_easypp")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn help_exposes_nested_commands_and_command_specific_options() {
    let p = Project::new("help");
    assert!(p.ok(&[]).contains("easypp help deps add"));
    let dependencies = p.ok(&["deps", "--help"]);
    assert!(dependencies.contains("add") && dependencies.contains("install"));
    let add = p.ok(&["help", "deps", "add"]);
    assert!(add.contains("<NAME/VERSION>") && add.contains("--build-missing"));
    assert!(p.ok(&["test", "--help"]).contains("--timeout"));
    assert!(!p.ok(&["run", "--help"]).contains("--timeout"));
    assert!(p.fails(&["build", "-j0"]).contains("0"));
}

#[test]
fn new_project_builds_runs_and_tests() {
    let p = Project::new("new");
    assert!(p.ok(&["run"]).contains("Hello, world!"));
    assert!(p.ok(&["test"]).contains("1 passed; 0 failed"));
    assert!(p.exists("compile_commands.json"));
    assert!(p.exists(".clang-format") && p.exists(".clang-tidy"));
    assert!(p.ok(&["--version"]).starts_with("easypp "));
}

#[test]
fn links_a_chain_of_libraries_in_order() {
    let p = Project::new("chain");
    p.write("src/libs/c/c.hpp", "#pragma once\nint c();\n")
        .write("src/libs/c/c.cpp", "#include \"c.hpp\"\nint c() { return 1; }\n")
        .write("src/libs/b/b.hpp", "#pragma once\nint b();\n")
        .write("src/libs/b/b.cpp", "#include \"b.hpp\"\n#include \"c/c.hpp\"\nint b() { return c() + 1; }\n")
        .write("src/libs/a/a.hpp", "#pragma once\nint a();\n")
        .write("src/libs/a/a.cpp", "#include \"a.hpp\"\n#include \"b/b.hpp\"\nint a() { return b() + 1; }\n")
        .write(
            "src/bin/abc.cpp",
            "#include \"a/a.hpp\"\n#include <cstdio>\nint main() { std::printf(\"%d\\n\", a()); }\n",
        );
    assert!(p.ok(&["info"]).contains("links: a, b, c"));
    assert!(p.ok(&["run", "abc"]).contains('3'));
}

#[test]
fn library_headers_need_their_folder() {
    let p = Project::new("include");
    p.write(
        "src/bin/hello.cpp",
        "#include \"greeting.hpp\"\nint main() { return 0; }\n",
    );
    let out = p.fails(&["build"]);
    assert!(
        out.contains("write #include \"greeting/greeting.hpp\""),
        "{out}"
    );
}

#[test]
fn library_cycles_are_errors() {
    let p = Project::new("cycle");
    p.write("src/libs/x/x.hpp", "#pragma once\n#include \"y/y.hpp\"\n")
        .write("src/libs/y/y.hpp", "#pragma once\n#include \"x/x.hpp\"\n");
    assert!(
        p.fails(&["build"])
            .contains("circular dependency between libraries")
    );
}

#[test]
fn skips_includes_for_other_platforms() {
    let p = Project::new("cond");
    p.write(
        "src/bin/hello.cpp",
        "#ifdef _WIN32\n#include <windows.h>\n#include \"nope/nope.hpp\"\n#endif\nint main() { return 0; }\n",
    );
    p.ok(&["build"]);
}

#[test]
fn removes_stale_outputs() {
    let p = Project::new("stale");
    p.write("src/bin/other.cpp", "int main() { return 0; }\n");
    p.ok(&["build"]);
    assert!(p.exists("build/debug/bin/other"));
    p.remove("src/bin/other.cpp");
    assert!(p.ok(&["build"]).contains("Removed"));
    assert!(!p.exists("build/debug/bin/other"));
    assert!(p.exists("build/debug/bin/hello"));
}

#[test]
fn debug_builds_catch_memory_errors() {
    let p = Project::new("asan");
    p.write(
        "src/bin/hello.cpp",
        "#include <vector>\nint main(int argc, char**) { std::vector<int> v(2); return v.data()[argc + 1]; }\n",
    );
    let out = p.fails(&["run"]);
    assert!(
        out.contains("AddressSanitizer: heap-buffer-overflow"),
        "{out}"
    );
    // Release builds have no sanitizers.
    p.ok(&["build", "--release"]);
}

#[test]
fn explains_undefined_references() {
    let p = Project::new("hints");
    p.write("src/libs/m/m.hpp", "#pragma once\nint twice(int);\n")
        .write("src/libs/m/m.cpp", "#include \"m.hpp\"\n")
        .write(
            "src/bin/hello.cpp",
            "#include \"m/m.hpp\"\nint main() { return twice(1); }\n",
        );
    let out = p.fails(&["build"]);
    assert!(
        out.contains("is declared in src/libs/m/m.hpp but no .cpp file defines it"),
        "{out}"
    );
}

#[test]
fn tests_run_in_parallel_with_a_timeout() {
    let p = Project::new("tests");
    p.write("src/tests/fails_test.cpp", "#include <cstdio>\nint main() { std::puts(\"boom\"); return 1; }\n")
        .write(
            "src/tests/hangs_test.cpp",
            "#include <thread>\nint main() { for (;;) std::this_thread::sleep_for(std::chrono::seconds(1)); }\n",
        );
    let out = p.fails(&["test", "--timeout", "2"]);
    assert!(out.contains("hangs_test ... TIMEOUT"), "{out}");
    assert!(
        out.contains("---- fails_test (exit code 1) ----\nboom"),
        "{out}"
    );
    assert!(out.contains("1 passed; 2 failed"), "{out}");
}

#[test]
fn fmt_check_reports_and_fmt_fixes() {
    if !have("clang-format") {
        return;
    }
    let p = Project::new("fmt");
    p.ok(&["fmt", "--check"]);
    p.write(
        "src/bin/hello.cpp",
        "int main(){if(true)return 0;return 1;}\n",
    );
    p.fails(&["fmt", "--check"]);
    p.ok(&["fmt"]);
    p.ok(&["fmt", "--check"]);
    let code = fs::read_to_string(p.dir.join("src/bin/hello.cpp")).unwrap();
    assert!(code.contains("if (true) {"), "{code}");
}

#[test]
fn export_cmake_builds() {
    if !have("cmake") || !have("ninja") {
        return;
    }
    let p = Project::new("cmake");
    p.ok(&["export-cmake"]);
    p.fails(&["export-cmake"]); // no overwrite without --force
    let cmake = |args: &[&str]| {
        let out = Command::new("cmake")
            .args(args)
            .current_dir(&p.dir)
            .env("CC", "clang")
            .env("CXX", "clang++")
            .output()
            .unwrap();
        assert!(out.status.success(), "cmake {args:?}:\n{}", text(&out));
    };
    cmake(&["-S", ".", "-B", "build-cmake", "-G", "Ninja"]);
    cmake(&["--build", "build-cmake"]);
    let out = Command::new("ctest")
        .args(["--test-dir", "build-cmake"])
        .current_dir(&p.dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
}

#[test]
fn examples_build_and_pass_their_tests() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut examples: Vec<_> = fs::read_dir(examples)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("src").is_dir())
        .collect();
    examples.sort();
    assert!(!examples.is_empty());
    let mut failures = Vec::new();
    for example in examples {
        let name = example.file_name().unwrap().to_string_lossy();
        eprintln!("Checking example {name}");
        let project = Project::copy_example(&example);
        let mut commands = vec![vec!["build", "-j4"], vec!["test", "-j4"]];
        if project.exists("conanfile.txt") {
            for args in &mut commands {
                args.push("--release");
            }
        }
        if matches!(name.as_ref(), "blockfall" | "life" | "asio-kv" | "qt-tasks") {
            if have("clang-format") {
                commands.push(vec!["fmt", "--check"]);
            }
            if have("clang-tidy") {
                commands.push(vec!["lint", "-j4"]);
            }
        }
        for args in &commands {
            let out = project.run(args);
            if !out.status.success() {
                failures.push(format!("{name}: easypp {args:?}\n{}", text(&out)));
                break;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn cuda_setup_errors_explain_how_to_fix_the_toolchain() {
    use std::os::unix::fs::PermissionsExt;

    let p = Project::new("cuda-setup");
    p.remove("src");
    p.write("src/bin/demo.cu", "int main() { return 0; }\n");
    fs::create_dir_all(p.dir.join("toolkit/bin")).unwrap();
    let build = || {
        let out = Command::new(easypp())
            .args(["build", "--release"])
            .current_dir(&p.dir)
            .env("PATH", p.dir.join("toolkit/bin"))
            .output()
            .unwrap();
        assert!(!out.status.success(), "{}", text(&out));
        text(&out)
    };
    let missing = build();
    assert!(
        missing.contains("Install the toolkit or add its bin directory to PATH"),
        "{missing}"
    );
    p.write(
        "toolkit/bin/nvcc",
        r#"#!/bin/sh
if [ "$1" = --list-gpu-code ]; then
  printf 'sm_75\n'
  exit 0
fi
printf 'crt/host_config.h:42: error: #error -- unsupported GNU version!\n' >&2
exit 1
"#,
    );
    fs::set_permissions(
        p.dir.join("toolkit/bin/nvcc"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let incomplete = build();
    assert!(
        incomplete.contains("missing cuda_runtime.h")
            && incomplete.contains("CUDA development files"),
        "{incomplete}"
    );
    p.write("toolkit/include/cuda_runtime.h", "");
    let incomplete = build();
    assert!(
        incomplete.contains("missing libcudart.so")
            && incomplete.contains("CUDA development files"),
        "{incomplete}"
    );
    p.write("toolkit/lib64/libcudart.so", "");
    let unsupported = build();
    assert!(
        unsupported.contains("unsupported GNU version"),
        "{unsupported}"
    );
    assert!(unsupported.contains("set NVCC_CCBIN"), "{unsupported}");
}

#[test]
fn cuda_builds_a_library_and_rebuilds_when_a_header_changes() {
    use std::os::unix::fs::PermissionsExt;

    if !have("nvcc") {
        eprintln!("Skipping CUDA smoke test: nvcc is not on PATH");
        return;
    }
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cuda");
    let p = Project::copy_example(&fixture);
    assert!(p.ok(&["info"]).contains("choose on first CUDA build"));
    let output = Command::new("nvcc")
        .arg("--list-gpu-code")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let capability: u16 = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .filter_map(|code| code.strip_prefix("sm_")?.parse().ok())
        .min()
        .unwrap();
    let architecture = format!("sm_{capability}");
    p.write(
        "tools/nvidia-smi",
        &format!(
            "#!/bin/sh\nprintf 'Fixture GPU, {}.{}\\n'\n",
            capability / 10,
            capability % 10
        ),
    );
    fs::set_permissions(
        p.dir.join("tools/nvidia-smi"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let path = std::env::join_paths(
        std::iter::once(p.dir.join("tools"))
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let first = Command::new(easypp())
        .args(["run", "-j4", "-v"])
        .current_dir(&p.dir)
        .env("PATH", path)
        .env_remove("CUDA_VISIBLE_DEVICES")
        .output()
        .unwrap();
    assert!(first.status.success(), "{}", text(&first));
    let first = text(&first);
    assert!(first.contains("CUDA runtime"), "{first}");
    assert!(first.contains(&format!("-arch={architecture}")), "{first}");
    assert!(
        p.ok(&["info", "--no-cache"])
            .contains(&format!("CUDA architecture: {architecture}"))
    );
    assert!(!p.ok(&["build", "-j4"]).contains("Compiling"));
    p.write(
        "src/libs/device/answer.cuh",
        "#pragma once\nconstexpr int answer = 40 + 2;\n",
    );
    let rebuilt = p.ok(&["run", "-j4"]);
    assert!(rebuilt.contains("Compiling src/libs/device/device.cu"));
    assert!(rebuilt.contains("CUDA runtime"));
    if have("cmake") && have("ninja") {
        p.ok(&["export-cmake"]);
        let cmake = fs::read_to_string(p.dir.join("CMakeLists.txt")).unwrap();
        assert!(cmake.contains(&format!("CMAKE_CUDA_ARCHITECTURES {capability}")));
        for args in [
            &["-S", ".", "-B", "build-cmake", "-G", "Ninja"][..],
            &["--build", "build-cmake", "-j4"],
        ] {
            let out = Command::new("cmake")
                .args(args)
                .current_dir(&p.dir)
                .env("CC", "clang")
                .env("CXX", "clang++")
                .output()
                .unwrap();
            assert!(out.status.success(), "cmake {args:?}:\n{}", text(&out));
        }
        let out = Command::new(p.dir.join("build-cmake/demo"))
            .output()
            .unwrap();
        assert!(
            out.status.success() && text(&out).contains("CUDA runtime"),
            "{}",
            text(&out)
        );
    }
}

#[test]
#[ignore = "requires Qt 6 base development tools"]
fn qt_designer_form_with_multiline_root_builds() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/qt-tasks");
    let p = Project::copy_example(&example);
    let form = "src/libs/window/mainwindow.ui";
    let text = fs::read_to_string(p.dir.join(form)).unwrap();
    p.write(form, &text.replace("<ui version", "<ui\n version"));
    p.ok(&["build", "-j4"]);
    assert!(p.exists("build/debug/bin/tasks"));
}

#[test]
#[ignore = "requires Qt 6 declarative development tools and Qt Quick runtime modules"]
fn qt_qml_resources_build_and_regenerate() {
    let p = Project::new("qt-qml");
    p.remove("src");
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/qt-qml/src");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(example)
            .arg(&p.dir)
            .status()
            .unwrap()
            .success()
    );
    let source = "src/bin/qml-counter/main.cpp";
    let main = fs::read_to_string(p.dir.join(source)).unwrap();
    let checks = r#"    if (!QFile::exists(label_resource) || !QFile::exists(":/custom/Legacy.qml")
        || QFile::exists(":/Legacy.qml")) {
        return 1;
    }
    QQmlApplicationEngine engine;"#;
    let main = main.replace("    QQmlApplicationEngine engine;", checks);
    p.write(
        source,
        &format!("#include <QtCore/QFile>\n#include \"labels/labels.hpp\"\n{main}"),
    );
    p.write(
        "src/libs/labels/labels.hpp",
        "#pragma once\ninline constexpr auto label_resource = \":/libs/labels/Label.qml\";\n",
    );
    p.write("src/libs/labels/Label.qml", "import QtQuick\nText {}\n");
    p.write(
        "src/bin/qml-counter/Legacy.qml",
        "import QtQuick\nItem {}\n",
    );
    p.write(
        "src/bin/qml-counter/custom.qrc",
        "<RCC><qresource prefix=\"/custom\"><file>Legacy.qml</file></qresource></RCC>\n",
    );

    let run = |command: &Path, args: &[&str]| {
        let out = Command::new(command)
            .args(args)
            .current_dir(&p.dir)
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("CC", "clang")
            .env("CXX", "clang++")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{command:?} {args:?}:\n{}",
            text(&out)
        );
        text(&out)
    };
    let build_and_run = || run(Path::new(easypp()), &["run", "--release", "--", "--check"]);
    assert!(build_and_run().contains("rcc (QML)"));
    assert!(!build_and_run().contains("rcc (QML)"));
    p.write(
        "src/bin/qml-counter/qml/labels.js",
        "function counterText(count) { return ['Count:', count].join(' ') }\n",
    );
    assert!(build_and_run().contains("rcc (QML)"));
    assert!(!build_and_run().contains("rcc (QML)"));

    if have("cmake") && have("ninja") {
        p.ok(&["export-cmake"]);
        run(
            Path::new("cmake"),
            &[
                "-S",
                ".",
                "-B",
                "build-cmake",
                "-G",
                "Ninja",
                "-DCMAKE_BUILD_TYPE=Release",
            ],
        );
        run(Path::new("cmake"), &["--build", "build-cmake"]);
        run(Path::new("./build-cmake/qml-counter"), &["--check"]);
    }
}

#[test]
#[ignore = "requires Qt 6 declarative tools, Qt Quick, and clang-tidy"]
fn qt_qml_named_module_registers_and_lints_cpp_types() {
    let p = Project::new("qt-qml-module");
    p.remove("src");
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/qt-qml-module/src");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(example)
            .arg(&p.dir)
            .status()
            .unwrap()
            .success()
    );
    let run = |command: &Path, args: &[&str]| {
        let out = Command::new(command)
            .args(args)
            .current_dir(&p.dir)
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("CC", "clang")
            .env("CXX", "clang++")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{command:?} {args:?}:\n{}",
            text(&out)
        );
        text(&out)
    };
    let easypp = Path::new(easypp());
    let build = || run(easypp, &["run", "--release", "--", "--check"]);
    assert!(build().contains("qmltyperegistrar"));
    assert!(!build().contains("Generating"));
    let header = "src/bin/qml-counter/Counter.hpp";
    let contents = fs::read_to_string(p.dir.join(header)).unwrap();
    p.write(
        header,
        &contents.replace("QML_ELEMENT", "QML_NAMED_ELEMENT(Counter)"),
    );
    assert!(build().contains("qmltyperegistrar"));
    assert!(run(easypp, &["lint", "--release"]).contains("QML in qml-counter"));
    if have("cmake") && have("ninja") {
        p.ok(&["export-cmake"]);
        run(
            Path::new("cmake"),
            &[
                "-S",
                ".",
                "-B",
                "build-cmake",
                "-G",
                "Ninja",
                "-DCMAKE_BUILD_TYPE=Release",
            ],
        );
        run(Path::new("cmake"), &["--build", "build-cmake"]);
        run(Path::new("./build-cmake/qml-counter"), &["--check"]);
    }
    let main = "src/bin/qml-counter/qml/Main.qml";
    let contents = fs::read_to_string(p.dir.join(main)).unwrap();
    p.write(
        main,
        &contents.replace("counter.value", "counter.missingProperty"),
    );
    let diagnostics = p.fails(&["lint", "--release"]);
    assert!(
        diagnostics.contains("src/bin/qml-counter/qml/Main.qml"),
        "{diagnostics}"
    );
    assert!(diagnostics.contains("missingProperty"), "{diagnostics}");

    p.write(main, &format!("import Easypp.Fixture.Missing\n{contents}"));
    let out = Command::new(easypp)
        .args(["run", "--release", "--", "--check"])
        .current_dir(&p.dir)
        .env("QT_QPA_PLATFORM", "offscreen")
        .env_remove("QT_FORCE_STDERR_LOGGING")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let diagnostics = text(&out);
    assert!(
        diagnostics.contains("module \"Easypp.Fixture.Missing\" is not installed"),
        "{diagnostics}"
    );
    assert!(
        diagnostics.contains("QML module 'Easypp.Fixture.Missing' was unavailable at runtime"),
        "{diagnostics}"
    );
    assert!(diagnostics.contains("QML_IMPORT_PATH"), "{diagnostics}");
}

/// Exercises real KConfig output, including its generated Q_OBJECT class,
/// transitive library headers, schema edits and recovery of either output.
#[test]
#[ignore = "requires Qt6 and KDE Frameworks 6 development tools"]
fn kde_kconfig_incremental_build() {
    let p = Project::new("kconfig");
    p.write(
        "src/libs/prefs/options.kcfgc",
        "File=options.kcfg\nClassName=Options\nSingleton=true\nGenerateProperties=true\n",
    );
    let schema = |n| {
        format!(
            r#"<?xml version="1.0"?><kcfg xmlns="http://www.kde.org/standards/kcfg/1.0"><kcfgfile name="easypp-test"/><group name="General"><entry name="Count" type="Int"><default>{n}</default></entry></group></kcfg>"#
        )
    };
    p.write("src/libs/prefs/options.kcfg", &schema(7));
    p.write(
        "src/libs/prefs/prefs.h",
        "#pragma once\n#include \"options.h\"\nint count();\n",
    );
    p.write(
        "src/libs/prefs/prefs.cpp",
        "#include \"prefs.h\"\nint count() { return Options::count(); }\n",
    );
    p.write("src/bin/check.cpp", "#include <QCoreApplication>\n#include <cstdlib>\n#include \"prefs/prefs.h\"\nint main(int argc, char **argv) { QCoreApplication app(argc, argv); return count() == std::atoi(argv[1]) ? 0 : 1; }\n");
    let first = p.ok(&["run", "check", "--", "7"]);
    assert!(first.contains("kconfig"), "{first}");
    assert!(!p.ok(&["build"]).contains("Generating"));
    p.write("src/libs/prefs/options.kcfg", &schema(9));
    assert!(p.ok(&["run", "check", "--", "9"]).contains("Generating"));
    p.remove("build/debug/gen/libs/prefs/options.h");
    assert!(p.ok(&["run", "check", "--", "9"]).contains("Generating"));
    p.remove("build/debug/gen/libs/prefs/options.cpp");
    assert!(p.ok(&["run", "check", "--", "9"]).contains("Generating"));
    p.write(
        "src/libs/prefs/options.kcfgc",
        "File=missing.kcfg\nClassName=Options\n",
    );
    assert!(p.fails(&["build"]).contains("does not exist"));
}

#[test]
#[ignore = "requires GLib development tools"]
fn glib_resources_build_and_regenerate() {
    let p = Project::new("glib-resources");
    p.remove("src/bin/hello.cpp");
    p.write("src/bin/check/app.gresource.xml",
        "<gresources><gresource prefix=\"/demo\"><file>message &amp; data.txt</file></gresource></gresources>\n");
    let resource = "src/bin/check/message & data.txt";
    p.write(resource, &"a".repeat(6_000));
    p.write(
        "src/bin/check/main.c",
        r#"
#include <gio/gio.h>
int main(void) {
    GBytes *bytes = g_resources_lookup_data("/demo/message & data.txt", 0, NULL);
    if (!bytes) return 1;
    g_bytes_unref(bytes);
    return 0;
}
"#,
    );
    assert!(p.ok(&["run", "check"]).contains("glib-compile-resources"));
    assert!(!p.ok(&["build"]).contains("Generating"));
    p.write(resource, &"b".repeat(6_000));
    assert!(p.ok(&["run", "check"]).contains("glib-compile-resources"));
    assert!(!p.ok(&["build"]).contains("Generating"));

    if have("cmake") && have("ninja") {
        p.ok(&["export-cmake"]);
        let cmake = |args: &[&str]| {
            let out = Command::new("cmake")
                .args(args)
                .current_dir(&p.dir)
                .env("CC", "clang")
                .env("CXX", "clang++")
                .output()
                .unwrap();
            assert!(out.status.success(), "cmake {args:?}:\n{}", text(&out));
            text(&out)
        };
        cmake(&["-S", ".", "-B", "build-cmake", "-G", "Ninja"]);
        assert!(cmake(&["--build", "build-cmake"]).contains("Generating"));
        assert!(!cmake(&["--build", "build-cmake"]).contains("Generating"));
        p.write(resource, &"c".repeat(6_000));
        assert!(cmake(&["--build", "build-cmake"]).contains("Generating"));
    }
}

#[test]
fn discovers_conan_metadata_for_the_selected_profile() {
    let p = Project::new("conan-pc");
    p.remove("src/bin/hello.cpp");
    p.write(
        "src/bin/probe.cpp",
        "#include <easypp_probe.h>\nint main() { return probe() == 42 ? 0 : 1; }\n",
    );
    let prefix = p.dir.join("dependency with spaces");
    p.write(
        "dependency with spaces/include/easypp_probe.h",
        "extern \"C\" int probe(void);\n",
    );
    p.write(
        "dependency with spaces/probe.c",
        "int probe(void) { return 42; }\n",
    );
    assert!(
        Command::new("clang")
            .args(["-shared", "-fPIC", "probe.c", "-o", "libeasypp_probe.so"])
            .current_dir(&prefix)
            .status()
            .unwrap()
            .success()
    );
    let pc = "build/conan/release/easypp-probe.pc";
    let metadata = format!(
        "prefix={}\nName: easypp-probe\nDescription: Conan metadata fixture\nVersion: 1\nCflags: -I\"${{prefix}}/include\"\nLibs: -L\"${{prefix}}\" -leasypp_probe\n",
        prefix.display()
    );
    // Another component can expose the same include directory.
    p.write(
        "build/conan/release/another-component.pc",
        &metadata.replace("-leasypp_probe", "-lwrong_component"),
    );
    p.write(pc, &metadata);
    assert!(
        p.ok(&["info", "--release"])
            .contains("easypp-probe (Conan)")
    );
    p.ok(&["run", "probe", "--release"]);
    assert!(!p.ok(&["info"]).contains("easypp-probe (Conan)"));
    p.write(
        pc,
        &metadata.replace("-leasypp_probe", "-lmissing_easypp_probe"),
    );
    assert!(
        p.fails(&["build", "--release"])
            .contains("missing_easypp_probe")
    );
}

#[test]
fn build_installs_missing_and_changed_conan_dependencies() {
    use std::os::unix::fs::PermissionsExt;

    let p = Project::new("conan-auto-install");
    p.write("conanfile.txt", "[requires]\nfixture/1\n");
    // Stand in for Conan to check install calls without a remote or global profile.
    p.write(
        "tools/conan",
        r#"#!/bin/sh
set -eu
if [ "$1" = profile ]; then exit 0; fi
[ "$1" = install ]
[ "$2" = conanfile.txt ]
while [ "$1" != --output-folder ]; do shift; done
mkdir -p "$2"
printf 'Name: fixture\nDescription: test\nVersion: 1\n' >"$2/fixture.pc"
printf 'install\n' >>conan-calls
"#,
    );
    let conan = p.dir.join("tools/conan");
    fs::set_permissions(conan, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(p.dir.join("tools"))
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let build = || {
        let out = Command::new(easypp())
            .args(["build", "--release"])
            .current_dir(&p.dir)
            .env("PATH", &path)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", text(&out));
    };
    let calls = || {
        fs::read_to_string(p.dir.join("conan-calls"))
            .unwrap()
            .lines()
            .count()
    };
    build();
    assert_eq!(calls(), 1);
    build();
    assert_eq!(calls(), 1);
    p.write("conanfile.txt", "[requires]\nfixture/2\n");
    build();
    assert_eq!(calls(), 2);

    p.write("conan.lock", "first revision");
    p.write(
        "build/conan/.install/packages/incomplete.pc",
        "partial install",
    );
    build();
    assert_eq!(calls(), 3);
    build();
    assert_eq!(calls(), 3);
    p.write("conan.lock", "second revision");
    build();
    assert_eq!(calls(), 4);
    p.remove("conan.lock");
    build();
    assert_eq!(calls(), 5);
}

#[test]
fn named_modules_link_and_rebuild_importers() {
    let p = Project::new("cpp-modules");
    p.remove("src");
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/modules/src");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(example)
            .arg(&p.dir)
            .status()
            .unwrap()
            .success()
    );
    let first = p.ok(&["run", "--release", "-j4"]);
    assert!(first.lines().any(|line| line == "35"), "{first}");
    assert!(!p.ok(&["build", "--release"]).contains("Compiling"));

    let interface = "src/libs/arithmetic/operations.cppm";
    p.write(
        interface,
        "export module arithmetic:operations;\nexport int double_value(int n) { return 3*n; }\n",
    );
    let updated = p.ok(&["run", "--release", "-j4"]);
    assert!(updated.lines().any(|line| line == "49"), "{updated}");
    assert!(updated.contains("Compiling src/bin/demo.cpp"), "{updated}");

    let bmi = "build/release/obj/libs/arithmetic/operations.cppm.o.pcm";
    p.remove(bmi);
    p.ok(&["build", "--release"]);
    assert!(p.exists(bmi));

    p.write(
        interface,
        "export module arithmetic:operations;\nimport report;\n",
    );
    assert!(
        p.fails(&["build", "--release"])
            .contains("module import cycle")
    );
}

#[test]
fn header_units_rebuild_and_share_with_named_modules() {
    let p = Project::new("header-units");
    p.remove("src");
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/header-units/src");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(example)
            .arg(&p.dir)
            .status()
            .unwrap()
            .success()
    );
    p.write(
        "src/libs/scale/arrays.cppm",
        "export module arrays;\nexport import <array>;\n",
    );
    let source = "src/bin/demo/main.cpp";
    let main = fs::read_to_string(p.dir.join(source))
        .unwrap()
        .replace("import <array>;", "import \"array\";");
    p.write(source, &format!("import arrays;\n{main}"));
    let header = "src/bin/demo/input.hpp";
    p.write(header, "inline constexpr int input = 21;\n");
    p.write("src/bin/demo/constants.hpp", "#include \"input.hpp\"\n");

    let first = p.ok(&["run", "--release", "-j4"]);
    assert_eq!(first.matches("Precompiling header unit array").count(), 1);
    assert!(!p.ok(&["build", "--release"]).contains("Precompiling"));

    p.write(header, "inline constexpr int input = 20 + 1;\n");
    let updated = p.ok(&["run", "--release", "-j4"]);
    assert!(updated.contains("Precompiling header unit constants.hpp"));
    assert!(updated.contains("Compiling src/bin/demo/main.cpp"));
    assert!(!updated.contains("Precompiling header unit array"));

    assert!(p.fails(&["export-cmake"]).contains("header units"));
}

#[test]
#[ignore = "requires a Clang-compatible standard library module manifest"]
fn standard_library_modules_build_on_demand() {
    let p = Project::new("std-module");
    p.remove("src");
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/std-module/src");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(example)
            .arg(&p.dir)
            .status()
            .unwrap()
            .success()
    );

    let demo = p.ok(&["run", "--release", "-j4"]);
    assert!(demo.lines().any(|line| line == "42"), "{demo}");
    let outputs = || {
        fs::read_dir(p.dir.join("build/release/obj/standard-modules"))
            .unwrap()
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "pcm"))
            .count()
    };
    assert_eq!(outputs(), 1);
    assert!(p.ok(&["test", "--release", "-j4"]).contains("1 passed"));
    assert_eq!(outputs(), 2);
    assert!(!p.ok(&["build", "--release"]).contains("Compiling"));
}

#[test]
#[ignore = "requires clang-scan-deps"]
fn module_scanner_follows_headers_and_compiler_changes_rebuild_modules() {
    use std::os::unix::fs::PermissionsExt;

    let p = Project::new("module-scanner");
    p.remove("src/bin/hello.cpp");
    p.write(
        "src/libs/math/math.cppm",
        "export module math;\nexport int answer() { return 42; }\n",
    );
    p.write(
        "src/bin/imports.hpp",
        "#ifdef NDEBUG\nimport math;\n#else\nimport debug_only;\n#endif\n",
    );
    p.write(
        "src/bin/check.cpp",
        "#include \"imports.hpp\"\nint main() { return answer() != 42; }\n",
    );
    p.ok(&["run", "--release"]);
    assert!(!p.ok(&["build", "--release"]).contains("Compiling"));
    assert!(p.fails(&["build"]).contains("debug_only"));

    let old = fs::read_to_string(p.dir.join("build/release/module-compiler")).unwrap();
    let compiler = Command::new("sh")
        .args(["-c", "command -v clang++"])
        .output()
        .unwrap();
    assert!(compiler.status.success());
    let compiler = std::str::from_utf8(&compiler.stdout).unwrap().trim();
    p.write(
        "tools/clang++",
        &format!("#!/bin/sh\nexec '{compiler}' \"$@\"\n"),
    );
    fs::set_permissions(
        p.dir.join("tools/clang++"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let original_path = std::env::var_os("PATH").unwrap();
    let path = std::env::join_paths(
        std::iter::once(p.dir.join("tools")).chain(std::env::split_paths(&original_path)),
    )
    .unwrap();
    let out = Command::new(easypp())
        .args(["build", "--release"])
        .current_dir(&p.dir)
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("Compiling src/libs/math/math.cppm"));
    assert!(text(&out).contains("Compiling src/bin/check.cpp"));
    assert_ne!(
        fs::read_to_string(p.dir.join("build/release/module-compiler")).unwrap(),
        old
    );
}
