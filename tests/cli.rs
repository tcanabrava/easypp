//! End-to-end tests: each one creates a project in a temporary folder and
//! drives the real `easypp` binary. They need clang (with its sanitizer
//! runtimes); `export_cmake_builds` also needs cmake and ninja.
//!
//! `cargo test -- --ignored` also builds and tests the examples, which need
//! SDL2, GTK4, OpenMP, Boost and Qt 6.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Project {
    dir: PathBuf,
}

impl Project {
    /// `easypp new` in a fresh temporary folder.
    fn new(test: &str) -> Project {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let parent = std::env::temp_dir().join(format!(
            "easypp-it-{}-{}-{}",
            std::process::id(),
            test,
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&parent).unwrap();
        let out = Command::new(easypp())
            .arg("new")
            .arg("p")
            .current_dir(&parent)
            .output()
            .unwrap();
        assert!(out.status.success(), "easypp new failed: {}", text(&out));
        Project {
            dir: parent.join("p"),
        }
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

/// Needs SDL2, GTK4, OpenMP, Boost and Qt 6: `cargo test -- --ignored`.
/// The examples are what students read, so they must also be formatted and
/// lint-clean.
#[test]
#[ignore]
fn examples_build_and_pass_their_tests() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut commands = vec![vec!["test"]];
    if have("clang-format") {
        commands.push(vec!["fmt", "--check"]);
    }
    if have("clang-tidy") {
        commands.push(vec!["lint"]);
    }
    for example in ["blockfall", "life", "asio-kv", "qt-tasks"] {
        for args in &commands {
            let out = Command::new(easypp())
                .args(args)
                .current_dir(examples.join(example))
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{example}: easypp {args:?}\n{}",
                text(&out)
            );
        }
    }
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
