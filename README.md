# easypp

A zero-configuration build tool for small C and C++ projects. There is no
build file: the folder layout is the configuration.

```text
project_root/
    src/
        thirdparty/          third-party headers, added with -isystem
        libs/
            static_lib1/     -> build/<profile>/lib/libstatic_lib1.a
            static_lib2/
        bin/
            first_exec.cpp   -> build/<profile>/bin/first_exec
            second_exec/     a folder = one multi-file executable
        tests/
            first_test.cpp   -> passes when it exits with 0
```

```sh
easypp new hello && cd hello
easypp run                # build and run (pick one with `easypp run <name>`)
easypp test               # build and run src/tests: in parallel, 60 s limit each
easypp debug [name]       # build and start a program or test under lldb
easypp fmt [--check]      # format with clang-format (--check: report only)
easypp lint                # check for common bugs with clang-tidy
easypp export-cmake       # write a CMakeLists.txt that builds the same way
easypp build --release    # -O2 -DNDEBUG instead of -O0 -g
easypp info               # what was discovered, and what links to what
easypp doctor             # check the toolchain and project dependencies
easypp clean
```

Run `easypp --help` to list commands. Use `easypp help run` for a command's
options or `easypp help deps add` for a nested command.

`easypp doctor` works before you create a project. Inside a project it also
checks the tools and libraries it can infer from the sources, including Qt,
OpenMP, CUDA, and Conan setup. It reports required fixes without creating
project build files or installing packages. Use `--release` to check the
Release profile.

## How it works

- **Libraries**: every folder in `src/libs/` is a static library made of all the
  `.c`/`.cpp` files inside it. A folder without sources is header-only.
- **Linking**: easypp reads the `#include`s. When `first_exec.cpp` includes
  `"static_lib1/lib1_header.hpp"`, it links `static_lib1`, plus whatever
  `static_lib1` includes from other libraries, in the right static-link order.
  Circular dependencies between libraries are reported as errors.
- **Include paths**: only `src/libs` is on the include path, so a library's
  header is always written with its folder: `#include "static_lib1/lib1_header.hpp"`.
  Inside a library, its own headers can be included as `"lib1_header.hpp"`
  (quoted includes are found next to the including file). Writing a bare
  `"lib1_header.hpp"` elsewhere gets a warning naming the right form. Each
  `src/thirdparty/<x>/` folder (or its `include/` sub-folder) goes on with
  `-isystem`, so warnings from third-party code don't break the build. A
  third-party folder with `.c`/`.cpp` files (in it or its `src/`) is compiled
  into a library, in the GNU dialect and without warnings, and linked into
  whatever includes its headers; files defining `main()` and folders such as
  `examples/` or `tests/` are left out. The
  system's default include and library folders are used as usual.
- **C++ named modules**: put interfaces and partitions (for example, `.cppm`
  files) next to the other sources in a library or multi-file executable.
  `import name;` discovers the provider and links its library. Clang builds
  compiled module interfaces before their importers, and interface changes
  invalidate dependent objects. See the [modules example](examples/modules/README.md).
  When source declarations identify a named-module project, easypp also uses
  `clang-scan-deps` if installed to discover imports in included headers and
  select imports from the active preprocessor branches. It falls back to
  source-text discovery for units Clang cannot scan before generation.
  Changing the Clang executable invalidates cached compiled interfaces.
  Named modules supplied by external packages are outside the current support.
- **Standard library modules**: `import std;` and `import std.compat;` use the
  module sources supplied by Clang's selected C++ standard library. Easypp
  discovers them from its installed JSON manifest and compiles only the
  modules a build needs. See the [standard modules example](examples/std-module/README.md).
  CMake export for these imports requires CMake 4.5 or newer and Ninja.
- **C++ header units**: `import "header.hpp";` and `import <array>;` compile the
  header as a Clang PCM. The same discovery rules used for includes determine
  library dependencies. Changes to the imported header or its textual includes
  rebuild the unit. See the [header units example](examples/header-units/README.md).
  Imports must be written directly in C++ source files. CMake export cannot
  export projects that use header units.
- **System libraries** (Linux): `#include` a system header and easypp
  links the library that provides it. It checks these rules in order:
  1. A pkg-config package named like the header: `<SDL2/SDL.h>` → `sdl2`,
     `<curl/curl.h>` → `libcurl`, `<GLFW/glfw3.h>` → `glfw3`, `<zlib.h>` → `zlib`.
  2. A package whose own include folder holds the header: `<SDL.h>` is in
     `/usr/include/SDL2`, `<gtk/gtk.h>` in `/usr/include/gtk-4.0`. If several
     packages match, it prefers the base, newest-looking one (gtk4).
  3. A `lib<name>.so`/`.a` in the compiler's library folders or `/usr/local/lib`,
     `/usr/lib64`, ...: `<pthread.h>` → `-lpthread`. For `<prefix/module...>`
     it tries `lib<prefix>_<module>` first, which covers compiled Boost
     libraries: `<boost/json.hpp>` → `-lboost_json`. Libraries outside the
     linker's default folders get `-L` and an rpath.
  4. `<math.h>` → `-lm`.
  5. OpenMP: `<omp.h>` or any `#pragma omp` line adds `-fopenmp` to compiling
     and linking.

  Header-only libraries (`<nlohmann/json.hpp>`) link nothing. The flags come
  from `pkg-config`, which also follows `Requires:`. Without `pkg-config` they
  are read from the `.pc` file. Package `-I` flags become `-isystem`. Each
  library or executable gets only the flags of the system libraries it uses,
  directly or through the internal libraries it uses. System libraries are linked with
  `-Wl,--as-needed`, so an unnecessary guess is dropped by the linker.
  `easypp info` shows what was picked.
- **Qt 6** (Qt 5 is not supported): `<QtWidgets/...>`, `<QtWidgets>` or a bare `<QPushButton>`
  links the right module, and pkg-config brings in the modules it needs (Core,
  Gui). Anything using Qt is compiled with `-fPIC`, which Qt requires. Code
  generation needs no setup either:
  - **moc**: a header with `Q_OBJECT`, `Q_GADGET` or `Q_NAMESPACE` gets
    `moc_<name>.cpp`, compiled into its library or program. A `.cpp` with
    `Q_OBJECT` gets `<name>.moc`, for `#include "<name>.moc"` at its end.
  - **uic**: `form.ui` gives `ui_form.h`, generated before anything compiles.
  - **rcc**: `res.qrc` gives `qrc_res.cpp`. For a library, the resources are
    linked straight into each program that uses it (in a static archive they
    would be dropped), and editing a file the `.qrc` lists regenerates it.
  - **QML**: a C++ executable directory containing `.qml` files gets an
    automatic resource bundle. For example, `src/bin/app/qml/Main.qml` is
    available at `qrc:/qml/Main.qml` to `QQmlApplicationEngine`. The bundle
    preserves relative paths and includes `.js`, `.mjs`, and `qmldir` files.
    Libraries use `qrc:/libs/<library>/` as their resource prefix. Use a
    `.qrc` for additional assets or custom aliases. CMake exports use the
    same resource URLs. See [the Qt QML example](examples/qt-qml).
    For automatic C++ type registration, put a `module Your.Uri` declaration
    in the executable's `qml/qmldir` and annotate C++ types in headers with
    `QML_ELEMENT` or `QML_NAMED_ELEMENT`. easypp registers the module at version
    1.0, embeds it under `qrc:/qt/qml/`, and runs `qmllint` for it through
    `easypp lint`. See the [named module example](examples/qt-qml-module).
    If a QML app fails to start because Qt reports a missing import,
    `easypp run` names the module and explains how to make it available.

  Generated files go to `build/<profile>/gen/`, which is on the include path of
  the component and of everything that uses it. The tools are found through
  pkg-config (`/usr/lib/qt6/moc` and so on), not PATH.
- **Compilers**: `.c` files use `clang -std=c17`; `.cpp`/`.cc`/`.cxx` files use
  `clang++ -std=c++23`. Executables that contain any C++ are linked with clang++.
  The warning flags are `-Wall -Wextra -pedantic -Werror`.
- **KDE Frameworks 6**: installed KDE imported-target metadata supplies include
  paths and public library dependencies when no pkg-config package is available.
  This reads literal system export properties, without invoking CMake or requiring
  a project build file. `.kcfgc` files are discovered beside sources: their `File=`
  schema is passed to `kconfig_compiler_kf6`, generating `.h`/`.cpp` files before
  moc and compilation. Schema/options edits and missing generated outputs trigger
  regeneration. Generated files stay under `build/<profile>/gen/`.
- **Runtime data**: put shared assets under `data/<application>/`.
  `easypp run` and `easypp debug` prepend the project's `data/` to `XDG_DATA_DIRS`.
  See `examples/kmines` for an imported KDE application using these conventions.
- **GLib resources**: a `.gresource.xml` manifest runs `glib-compile-resources`.
  Changes to listed files regenerate the C source, which links directly into
  executables so registration runs at startup. See `examples/vte` for the
  upstream VTE GTK 4 demo.
- **Moving on to CMake**: `easypp export-cmake` writes a CMakeLists.txt in
  which every folder is the target it was: libraries with their links and
  include paths, programs, CTest tests with the same timeout, pkg-config
  packages, OpenMP, Qt 6 (AUTOMOC/AUTOUIC/AUTORCC), the same warnings and the
  debug sanitizers. All the examples build and pass their tests with it.
- **Formatting and linting**: `easypp fmt` formats the project's own code
  (WebKit-like: four spaces, braces on every body, 120 columns) and
  `easypp lint` runs clang-tidy with checks that find real bugs (use after move,
  unchecked conversions, exceptions escaping, ...) without style nagging.
  `easypp new` writes these settings as `.clang-format` and `.clang-tidy`, so
  the editor agrees with the commands; a project's own files always win.
- **Tests**: each program in `src/tests` is a test that passes when it exits
  with 0. Tests run in parallel (`-j`), each with a time limit (`--timeout`,
  60 s by default), and only failing tests show their output. GoogleTest and
  Catch2 work out of the box: a test that includes them without defining
  `main()` is linked with `gtest_main` or `catch2-with-main`.
- **Errors explained**: after a compiler or linker error, easypp adds `help:`
  lines in plain language. A missing header gets the right `"library/header"`
  form, a note that Qt 5 is unsupported, or this distribution's command to find
  the package that provides it. An undefined reference is matched against the
  project: declared but never defined, defined with a different signature,
  a class member or destructor without a body, a missing `main()`, or a
  function from a library whose header is not included.
- **Sanitizers in debug builds**: debug builds use AddressSanitizer and
  UndefinedBehaviorSanitizer (`-fsanitize=address,undefined`, with undefined
  behaviour fatal). Out-of-bounds access, use-after-free, leaks and signed
  overflow stop the program with a report naming the source line. Leaks that
  GUI and system libraries (GTK, Qt, SDL, fontconfig, Mesa, ...) keep until
  exit on purpose are suppressed, so reports are about your code. Release
  builds have no sanitizers; `--no-sanitize` turns them off in debug.
- **Builds are incremental and parallel**: clang depfiles track headers,
  and anything whose flags changed is rebuilt.
  `compile_commands.json` is written at the project root (where clangd, VS Code
  and CLion look) on every build. `easypp watch` keeps it up to date while you
  edit: adding a file, a library or a system `#include` updates it within half
  a second, without building. In Qt projects it also runs moc, uic and rcc, so
  a new `ui_form.h` exists as soon as `form.ui` is saved.
- **Discovery cache**: `build/easypp.db` (an embedded Turso database) stores the
  compiler's search paths, the parsed pkg-config index, which system library
  each header maps to, and each source file's `#include` list. The first build
  fills it, which takes about 0.2 s longer. Later runs skip detection, so a
  no-op build takes about 10 ms instead of about 100 ms. The system part is
  redone when clang, pkg-config, `PKG_CONFIG_PATH` or any include, library or
  pkg-config folder changes. `--no-cache` ignores the cache, and `easypp clean`
  deletes it.

## Conan packages

With pkg-config and Clang installed, add a dependency from the project directory.
Easypp uses Conan 2 on PATH or installs it for your user with pipx or uv.
If no default Conan profile exists, easypp detects a local Clang profile.

```sh
easypp deps add zlib/1.3.1 --release
easypp build --release
```

`deps add` writes the reference to the `[requires]` section of `conanfile.txt`
after a successful install. Adding another version of the same package replaces
its reference. Check the manifest into version control. On a fresh checkout,
`easypp build --release` installs the recorded dependencies before compiling.
`run`, `test`, and `debug` do the same when their dependencies are missing or
the manifest has changed. These commands let Conan build missing binaries.
Use `easypp deps install --release` to install explicitly, with `--build-missing`
if needed.

Use ordinary includes such as `#include <zlib.h>`. Conan generates pkg-config
metadata under `build/conan/release/`. Omitting `--release` selects a separate
Debug installation from the same manifest. The manifest pins direct
requirements. Commit a Conan lockfile as well to pin the resolved recipe graph.
Configure exported CMake builds for the profile used to run `easypp export-cmake`.

See [the compressed JSON example](examples/conan-json/README.md) for zlib,
nlohmann/json, and GoogleTest in one project.

## Building easypp

```sh
cargo install --path .
```

Run `cargo test` from the repository root after installing the dependencies
listed in the [CI workflow](.github/workflows/ci.yml). The test suite
copies every project under `examples/` into a temporary directory, builds it
from scratch, and runs its tests. The Conan example installs the dependencies
recorded in its manifest, so a fresh Conan cache may need network access.
Use `cargo test -- --include-ignored` to also run the focused integration
tests that require additional tools.

## ARM64 Linux in QEMU

Install Clang, an ARM64 Linux GCC toolchain and sysroot, and QEMU user-mode
tools. On Arch Linux the additional packages are `aarch64-linux-gnu-gcc`
and `qemu-user`. Try the standard-library-only [C++ example](examples/qemu-cpp):

```sh
cd examples/qemu-cpp
easypp doctor --target aarch64-linux-gnu
easypp build --target aarch64-linux-gnu
easypp run --target aarch64-linux-gnu
easypp test --target aarch64-linux-gnu
```

The default sysroot is `/usr/aarch64-linux-gnu`. Pass `--sysroot DIR` to
use another compatible ARM64 Linux SDK and runtime root. easypp uses that
root for target `pkg-config` metadata and passes it to QEMU as the guest
loader prefix. Native and ARM64 outputs and discovery databases are separate.
`easypp clean --target aarch64-linux-gnu` removes the selected target's
outputs. Debug cross builds use debug information with sanitizers disabled.

This profile is for hosted Linux C/C++ programs running under QEMU user mode.
It covers project libraries and SDK packages. Conan, CUDA, C++ modules, Qt/KDE
code generation, remote debugging, and cross-target CMake export remain
outside this profile.

## CUDA

With the NVIDIA CUDA Toolkit and its supported host compiler installed, put
`nvcc` on `PATH`. easypp compiles `.cu` files as CUDA C++20. They can be
executables, tests, or part of a library under `src/libs/`. An executable that
uses one of these libraries links the CUDA runtime automatically. C and C++
files still compile with Clang and retain their Debug sanitizers.

On the first CUDA build, easypp asks for an architecture. The suggested value
uses the first visible NVIDIA GPU's compute capability and the targets that
`nvcc` supports. A noninteractive build accepts the recommendation
automatically. When GPU detection is unavailable, the initial choice is nvcc's
default architecture. The project database remembers the choice across
profiles and `--no-cache` builds. See it with `easypp info`; CMake exports
carry over a selected architecture. `easypp clean` removes the saved choice.

Use `easypp cuda-arch sm_86` to change the saved target without cleaning.
`easypp cuda-arch default` restores nvcc's default, and `easypp cuda-arch`
offers the GPU-based recommendation in a terminal. The next build recompiles
the affected CUDA sources and relinks their targets.

CUDA kernels can call device functions defined in other `.cu` files in the
target or its static libraries. easypp runs device linking before host linking
and carries this setup into CMake exports.

The [CUDA smoke project](tests/fixtures/cuda) builds without a GPU. Its
`--gpu` argument launches a kernel and checks the result on a machine with
a working NVIDIA driver. The CUDA smoke test runs with `cargo test` when
`nvcc` is on `PATH`, and checks CMake export when CMake and Ninja are installed.

The [CUDA, GTK 4, and OpenMP Game of Life](examples/life-cuda) combines a GPU
simulation with a desktop interface and a headless CPU comparison.
