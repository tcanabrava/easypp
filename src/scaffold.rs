//! `easypp new` / `easypp init`: writes the standard layout with a tiny example.

use std::fs;
use std::path::Path;

const DIRS: &[&str] = &[
    "src/thirdparty",
    "src/libs/greeting",
    "src/bin",
    "src/tests",
];

const FILES: &[(&str, &str)] = &[
    (".gitignore", "/build/\n/compile_commands.json\n"),
    (
        "src/thirdparty/README.md",
        "Put third-party code here, one folder per project.\n\
         \n\
         Each folder (or its `include/` sub-folder, if it has one) is added as a\n\
         *system* include path, so warnings inside third-party headers do not\n\
         break your `-Werror` build.\n\
         \n\
         A folder with .c/.cpp files (in the folder itself or in its src/) is\n\
         compiled into a library, without warnings, and linked into whatever\n\
         includes its headers. Files that define main() are skipped.\n",
    ),
    (
        "src/libs/greeting/greeting.hpp",
        r#"#pragma once

#include <string>

namespace greeting {

std::string hello(const std::string& name);

}
"#,
    ),
    (
        "src/libs/greeting/greeting.cpp",
        r#"#include "greeting.hpp"

namespace greeting {

std::string hello(const std::string& name)
{
    return "Hello, " + name + "!";
}

}
"#,
    ),
    (
        "src/bin/hello.cpp",
        r#"// Including a header from src/libs/greeting is all it takes:
// easypp notices it and links the greeting library for you.
#include "greeting/greeting.hpp"

#include <iostream>

int main(int argc, char* argv[])
{
    std::cout << greeting::hello(argc > 1 ? argv[1] : "world") << '\n';
}
"#,
    ),
    (
        "src/tests/greeting_test.cpp",
        r#"// Each file in src/tests is a test program: exit code 0 means it passed.
#include "greeting/greeting.hpp"

#include <cstdio>

int main()
{
    if (greeting::hello("world") != "Hello, world!") {
        std::fprintf(stderr, "unexpected greeting\n");
        return 1;
    }
    return 0;
}
"#,
    ),
];

/// Creates `dir` with the standard layout. Fails if `dir` already exists.
pub fn new_project(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        return Err(format!("'{}' already exists", dir.display()));
    }
    write_layout(dir)
}

/// Adds the standard layout to an existing folder that has no `src/` yet.
pub fn init(dir: &Path) -> Result<(), String> {
    if dir.join("src").exists() {
        return Err("src/ already exists here; `init` only sets up folders without one".into());
    }
    write_layout(dir)
}

fn write_layout(dir: &Path) -> Result<(), String> {
    for d in DIRS {
        let path = dir.join(d);
        fs::create_dir_all(&path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    }
    for (name, content) in FILES {
        let path = dir.join(name);
        if !path.exists() {
            fs::write(&path, content)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        }
    }
    Ok(())
}
