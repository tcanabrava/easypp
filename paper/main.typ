// Folders, Not Build Files: a zero-configuration C++ build tool for teaching — draft
// Build: typst compile main.typ
// Data: every number in Section 5 comes from eval/run.sh (output in eval/results.txt).

#set document(
  title: "Folders, Not Build Files: A Zero-Configuration C++ Build Tool for Teaching",
  author: "Tomaz Canabrava",
)
#set page(paper: "a4", margin: (x: 2.2cm, y: 2.5cm), numbering: "1")
#set text(font: "Libertinus Serif", size: 11pt, lang: "en")
#set par(justify: true)
#set heading(numbering: "1.1")
#show "C++": it => box(it)
#show link: set text(fill: rgb("#1a5fb4"))
#show table: set text(size: 9.5pt)
#show table: set par(justify: false)
#show raw.where(block: true): set text(size: 8.5pt)
#show raw.where(block: true): block.with(fill: luma(246), inset: 7pt, radius: 2pt, width: 100%)
#set table(stroke: 0.5pt + luma(180), inset: 5pt)

#let easypp = [_easypp_]

#align(center)[
  #text(17pt, weight: "bold")[Folders, Not Build Files]
  #v(0.2em)
  #text(13pt)[A Zero-Configuration C++ Build Tool for Teaching]
  #v(0.8em)
  Tomaz Canabrava
  #v(0.2em)
  #text(10pt, style: "italic")[Draft, November 2026]
]

#v(1em)

#block(inset: (x: 1.5em))[
  *Abstract.* A student who wants to split a C++ program into a library and an
  executable must first learn a build system. In Rust and Go that step does not
  exist: the tool that ships with the language finds the code by convention. We
  present #easypp, a small build tool for C and C++ that does the same. There is no
  build file: folders under `src/libs` are static libraries, files under `src/bin`
  are executables, files under `src/tests` are tests. Which libraries each program needs is worked
  out from its `#include` lines, including system libraries found through
  pkg-config. The defaults are fixed and strict: clang, C++23,
  `-Wall -pedantic -Werror`, and AddressSanitizer with UBSan in debug builds.
  Compiler and linker errors come with plain-language explanations, and a
  `CMakeLists.txt` can be exported for the move to industry tools. We compare it
  with CMake, Meson and a hand-written Makefile on a 12-target SDL2 game. The
  other three need 25 to 38 lines of build code to describe the same project. When a new library is added without editing
  that code, all three fail with a linker error and #easypp builds. With minimal
  set-up, CMake and Make silently compile a program that reads an uninitialised
  variable, using GNU C++17. #easypp rejects it. We argue that these defaults let a course teach
  libraries, headers and linking before it teaches a build language, and we are
  explicit about what the tool does not do.
]

= Introduction <sec-intro>

A companion study of C++ teaching in European universities found that
newcomers face a choice no other popular language imposes. They must pick a
compiler, a build system, a package manager and an editor, and nothing official
ties these into one path @canabrava2026. The language's "Get Started" page links to
services that no longer exist @isocpp-get-started. Some courses answer by teaching
inside an IDE that has its own project format, such as Code::Blocks with a bundled
compiler @mimuw-pk @wroclaw-cpp @kielce-po1. The JetBrains 2025 survey of C++
developers does not show Code::Blocks among the IDEs in use, so its share is at most
2% @jetbrains2025. Students thus learn a build workflow they will not meet at work. Courses that teach
industry practice instead must teach CMake, a second language with its own
vocabulary of targets, visibility keywords and generators @cmake-docs.

Rust shows that another path exists. `cargo new` creates a project, and Cargo
discovers binaries in `src/bin/` and tests in `tests/` without being told
@cargo-targets @rust-book. Compiler warnings are on by default. A student's first
multi-file program needs no build knowledge at all.

This paper asks how far the same idea goes in C++. Our contributions are:

+ #easypp, a build tool of about 6,000 lines of Rust that uses Clap for
  command-line parsing and an embedded database for the build cache. It derives the
  whole build from a fixed folder layout and from the `#include` lines in the
  sources, including system libraries, OpenMP, Boost and Qt 6 code generation
  (@sec-design).
+ A comparison of the main C++ build and project tools along the axes that matter
  to a beginner (@sec-landscape).
+ Measurements on a 12-target game against CMake, Meson and Make. We measure build-code
  size, the cost of a typical change, the behaviour of each tool's defaults, and
  build time (@sec-eval). All scripts and build files are published with the
  tool.
+ An argument, with its limits, for why this simplifies teaching (@sec-teaching,
  @sec-limits).

= The C++ build landscape <sec-landscape>

C++ has no build tool defined by the standard, so the ecosystem offers several
families. @tab-landscape summarises them from the point of view of someone
writing their first program with more than one file.

#figure({
  set text(size: 8.5pt)
  show raw: set text(size: 7.5pt)
  table(
    columns: (2.3cm, 1.1fr, 0.95fr, 0.85fr, 1.7fr, 0.95fr),
    align: left,
    table.header[*Tool*][*How the build is described*][*Warnings by default*][*Tests*][*Internal deps*][*New project*],
    [Make], [Makefile: rules, recipes, variables], [None (compiler default)], [By hand], [Listed by hand, link order too], [—],
    [CMake @cmake-docs], [CMake language; generates Make, Ninja or IDE projects], [None (measured, @sec-defaults)], [CTest], [`target_link_libraries`], [— (IDEs generate one)],
    [Meson @meson-options], [`meson.build`, a non-Turing-complete DSL], [`-Wall` (warning level 1)], [`meson test`], [`dependencies:` objects], [`meson init` @meson-commands],
    [Bazel @bazel-cc], [`BUILD` files in Starlark], [Toolchain-defined], [`bazel test`], [`deps = [...]`], [—],
    [xmake @xmake], [`xmake.lua` (Lua)], [Configurable per project], [Test targets], [`add_deps(...)`], [`xmake create`],
    [build2 @build2], [`buildfile` + package manifest], [Configurable], [`b test`], [`import` + `lib{}` prerequisites], [`bdep new`],
    [IDE projects (Code::Blocks, VS)], [XML project file edited through dialogs], [IDE template], [—/plug-ins], [Dialogs], [Wizard],
    [vcpkg @vcpkg, Conan @conan], [Package acquisition only; used together with a build system], [n/a], [n/a], [n/a], [Manifest],
    [Cargo (Rust) @cargo-targets], [`Cargo.toml` + folder conventions], [Lints on], [`cargo test`], [Modules: none; crates: `Cargo.toml`], [`cargo new`],
    [*#easypp*], [*Folder conventions only*], [*`-Wall`* \ *`-pedantic`* \ *`-Werror`*, *sanitizers in debug*], [*`easypp test`* (parallel, time limit)], [*Found from `#include`s*], [*`easypp new`*],
  )},
  caption: [Build and project tools from a beginner's point of view. CMake, Meson and Make were measured
  (@sec-eval); the other rows come from each tool's documentation.],
) <tab-landscape>

All of the general-purpose tools share one property. Even inside a single project,
the relationship "this program uses that library" is written twice: once in the
source as an `#include`, and once in the build description. Package managers do
not remove the second copy. They add a third place, the manifest, for external
packages. Every one of these tools rightly favours explicitness, because it has to scale
to large codebases with generated code, many platforms and many configurations.
A first-year course has none of those. Its projects are small and live on one
machine. Many of its students do not yet know what a linker is.

= Design of easypp <sec-design>

== One fixed layout

#easypp has no configuration file. A project is any folder whose `src/` contains
at least one of these sub-folders:

```text
project/
  src/
    libs/<name>/       static library <name>: every .c/.cpp inside it
    bin/<name>.cpp     executable <name>  (or a folder bin/<name>/ for several files)
    tests/<name>.cpp   test <name>: passes when it exits with status 0
    thirdparty/<name>/ third-party code: headers (-isystem), sources compiled
                       without warnings into a library
  build/               created by easypp; never edited
```

A library folder with no source files is a header-only library. `easypp new`
writes this layout with one library, one program and one test, plus
`.clang-format` and `.clang-tidy` files so the editor agrees with the tool. The
commands are `build`, `run`, `test`, `debug`, `info` (what was discovered),
`clean`, `fmt`, `lint`, `watch` (keeps the editor's compilation database current)
and `export-cmake`.

== Discovering dependencies from includes

The tool scans each library, executable and test for `#include` directives. It
resolves each include the way the compiler will, given the include paths #easypp
passes:

+ relative to the including file;
+ under `src/libs`, so a library's header is always written with its folder,
  `#include "board/board.hpp"`;
+ inside `src/thirdparty`.

Only `src/libs` is on the include path, never a library's own folder. A header is
therefore always named together with the library it belongs to, and two
libraries can both have a `utils.hpp` without ambiguity. Including a library's
header by its bare name gets a warning that names the include to write.

If the header belongs to library _L_, the component depends on _L_. Headers that
belong to no library, such as one next to an executable or in `thirdparty`, are
scanned in turn. So a dependency reached through a helper header is still found.
The library graph is sorted topologically. An executable links every library it
reaches, in dependents-first order, which is the order static linking requires.
A cycle between libraries is reported as an error that names the cycle.
#easypp treats it as a layering mistake to fix, not something to work around.

The scan is a line-based reading of `#include` directives, not a full
preprocessor. It does evaluate `#if`, `#ifdef` and `#elif` with three-valued
logic: macros known to be defined with clang on Linux are true, those of other
platforms and compilers (`_WIN32`, `__APPLE__`, `_MSC_VER`) are false, and
everything else is unknown. Only blocks that are certainly off are skipped, so
header guards and feature macros keep working. An `#include` written with a
macro cannot be followed and gets a warning. For student code this
approximation errs in the safe direction: the worst case is an extra library on
the link line.

== System libraries

On Linux, an include that is not found in the project is looked up in the
compiler's own search path. #easypp obtains that path by asking clang (`clang -v`,
`-print-search-dirs`), so `/usr/include`, `/usr/local/include`, `/usr/lib`,
`/usr/lib64` and multiarch folders are covered without a fixed list. The header
is then mapped to a library by three rules, tried in order:

+ A pkg-config package @pkg-config named like the header: `<SDL2/SDL.h>` maps to
  `sdl2`, `<curl/curl.h>` to `libcurl`, `<zlib.h>` to `zlib`.
+ A package whose private include folder contains the header. For example, `<SDL.h>`
  lives in `/usr/include/SDL2`. When several versions match, such as gtk2, gtk3
  and gtk4 for `<gtk/gtk.h>`, the latest always wins: the version is read from
  the folder's name (`gtk-4.0`), not from package names, where `gdk-x11-3.0` would
  read as version 11. There is no override, by design.
+ A `lib<name>.so` or `.a` with the header's name in the library folders, trying
  `lib<prefix>_<module>` first for `<prefix/module...>`. This covers Boost's
  compiled libraries, which ship no pkg-config files (`<boost/json.hpp>` links
  `-lboost_json`). A library outside the linker's defaults also gets `-L` and an
  rpath.

C++ standard library headers and compiler built-ins are excluded. `<math.h>` is
mapped to `-lm`, and `<omp.h>` or any `#pragma omp` line adds `-fopenmp`. A test
that includes GoogleTest or Catch2 without defining `main()` is linked with the
framework's `main` (`gtest_main`, `catch2-with-main`). Flags come from `pkg-config`, which also follows `Requires:`.
Its `-I` flags are turned into `-isystem`, so warnings inside system headers
cannot break a `-Werror` build. Each component gets only the flags of the system
libraries it uses, directly or through internal libraries. System libraries are
linked with `-Wl,--as-needed`, so a wrong guess is dropped by the linker at no cost.
Repeated libraries keep their last position on the link line, which static
linking requires.

*Qt 6.* `<QtWidgets/...>` and bare `<QPushButton>` map to the right Qt module, and
code using Qt is compiled with `-fPIC`, which Qt requires. Qt's code generators
run without configuration: a header or source with `Q_OBJECT` goes through moc, a
`.ui` form through uic, and a `.qrc` resource file through rcc. A library's
resources are linked straight into the programs that use it, because inside a
static archive the linker would drop them. Qt 5 is not supported.

*KDE Frameworks 6.* Some KDE libraries publish their dependency metadata only
as installed CMake target exports. #easypp reads their literal include paths,
compile definitions, library locations and public dependencies into the same
header index used for pkg-config. It does not execute project CMake files.
A `.kcfgc` file selects a KConfig schema through `File=`; the tool runs
`kconfig_compiler_kf6` before moc and compilation, tracking both generated
`.h` and `.cpp` outputs. This also covers generated classes requiring moc.
For runtime assets, `data/<application>/` mirrors a shared-data installation;
`run` and `debug` prepend `data/` to `XDG_DATA_DIRS`.

== A per-project cache <sec-cache>

Discovery has a fixed cost that does not depend on the project. On the evaluation
machine, four runs of clang to read its search paths take about 64 ms, and reading
the pkg-config index takes about 20 ms. #easypp therefore keeps a cache in
`build/easypp.db`, an embedded SQLite-compatible database (Turso). It stores the
system description, which library each system header maps to, each package's
flags, and the `#include` list of every project file. The system part is checked
against a fingerprint: the modification times of clang, clang++ and pkg-config and
of every include, library and pkg-config folder, plus `PKG_CONFIG_PATH`. Installing
or removing a package changes at least one of these, and then the system is
detected again. Include lists are reused while a file's modification time and size
are unchanged. Everything is read in one pass at start-up, and only changes are
written back, in one transaction.

The trade-off is deliberate. The first build of a project is about 0.2 s slower,
because it writes the cache. Every later run skips detection: a no-op rebuild of
the evaluation project drops from about 100 ms to about 10–17 ms. The cache can
never break a build. If it cannot be opened, for example because another #easypp
is running in the same project, the tool warns and works without it.
`easypp clean` deletes it with the rest of `build/`.

== Fixed, strict defaults

#figure(
  table(
    columns: (auto, 1fr),
    align: left,
    table.header[*Setting*][*Value*],
    [Compilers], [`clang` for `.c`, `clang++` for `.cpp/.cc/.cxx`; executables containing C++ link with `clang++`],
    [Language], [`-std=c17`, `-std=c++23` (no GNU extensions)],
    [Diagnostics], [`-Wall -pedantic -Werror` for the project's code; `-w` for third-party sources],
    [Profiles], [debug `-O0 -g` with AddressSanitizer and UBSan, undefined behaviour fatal (default); `--release` gives `-O2 -DNDEBUG`],
    [Third-party and system headers], [`-isystem`, so their warnings are not the student's problem],
    [Tests], [in parallel, 60 s limit each, output shown only for failures],
    [Editor support], [`compile_commands.json` @clang-cdb at the project root, kept current by `easypp watch`; `.clang-format` and `.clang-tidy` matching `easypp fmt` and `lint`],
    [Incrementality], [clang depfiles, a command stamp per output, parallel compilation, removal of stale outputs, a per-project discovery cache (@sec-cache)],
  ),
  caption: [What every #easypp project gets.],
) <tab-defaults>

Nothing in @tab-defaults can be changed from inside a project, except that
`--no-sanitize` builds debug without sanitizers. This is deliberate. For
teaching, a setting that exists is a setting someone must explain, and a warning
that can be silenced will be.

Debug executables also get a small generated file with LeakSanitizer
suppressions for libraries that keep memory until exit on purpose (GTK, GLib,
fontconfig, Qt, SDL, Mesa). A leak report blamed on them would teach students to
ignore the sanitizer. The four examples run with no sanitizer reports.

== Help around the build <sec-help>

*Errors explained.* After a failed compile or link, the tool's message is kept
and #easypp adds `help:` lines computed from the project. A missing header gets
the right `"library/header"` form, a note that Qt 5 is unsupported, or the
distribution's command for finding the package that provides it. An undefined
reference is matched against the project's files. It may be declared in a header
but defined nowhere, defined with a different signature, or a class member or
destructor without a body. It may be a C function called from C++ without
`extern "C"`, or a function from a library whose header is not included.

*Debugging, formatting, linting.* `easypp debug` starts a program under lldb,
LLVM's debugger, with sanitizer errors set to abort, so the debugger stops at the faulty
access. `easypp fmt` formats with clang-format, and `easypp lint` runs clang-tidy
with checks that find real bugs (use after move, unchecked conversions,
exceptions escaping) without style nagging.

*Moving on.* `easypp export-cmake` writes a `CMakeLists.txt` in which every
folder is the target it was: libraries with their links, programs, CTest tests,
pkg-config packages, OpenMP, Qt with AUTOMOC, AUTOUIC and AUTORCC, the same
warnings and the debug sanitizers. All four examples, a project with third-party
sources and one with GoogleTest and Catch2 build with the exported file and pass
`ctest`.

= Evaluation <sec-eval>

== Set-up

The test project is _Blockfall_, an original falling-block game written for this
evaluation and shipped with #easypp as an example. It has 7 internal libraries (2
of them header-only), 1 executable, 4 test programs and one system dependency, SDL2.
Its dependency structure is deliberately layered:
`render` → `game` → {`bag`, `board`} → `pieces`. The header-only `pixelfont` includes `<SDL2/SDL.h>` in its header, and `render.hpp` only forward-declares
SDL, so the tool must find the SDL dependency transitively.

For CMake, Meson and Make we wrote the shortest idiomatic build description we
could that reproduces #easypp's behaviour. That means the same flags, C++23,
sanitizers in debug builds, tests registered with the tool's test runner, and
SDL2 through pkg-config. The CMake version uses modern target-based commands. All
files are in `eval/`. `eval/run.sh` reruns every measurement from a fresh copy;
its output is `eval/results.txt`. The versions were clang 23.1.5, CMake 4.5.0 (Ninja
generator), Meson 1.12.1, Ninja 1.13.2 and GNU Make 4.4.1, on a 16-thread Linux machine.

== Build code a student must write <sec-size>

#figure(
  table(
    columns: (auto, auto, auto, 1fr),
    align: (left, right, right, left),
    table.header[*Tool*][*Lines*][*Words*][*Build-language vocabulary used*],
    [CMake], [38], [102], [17 commands, e.g. `add_library`, `target_link_libraries`, `target_include_directories`, `pkg_check_modules`, `enable_testing`, `add_test`; the `PUBLIC`/`PRIVATE`/`INTERFACE` keywords; imported targets (`PkgConfig::SDL2`)],
    [Meson], [25], [124], [9 functions (`static_library`, `declare_dependency`, `dependency`, `include_directories`, …) and 5 keyword arguments (`link_with:`, `dependencies:`, …)],
    [Make], [35], [171], [rules, pattern rules, automatic variables (`$@ $< $^`), `patsubst`/`call`/`shell`, depfile inclusion; library order written by hand],
    [#easypp], [0], [0], [the folder layout],
  ),
  caption: [Size of the build description for Blockfall (non-blank, non-comment lines).],
) <tab-size>

The build descriptions are short in absolute terms. What matters for teaching is
what each line asks the student to understand. The CMake file's 38 lines use 17
different commands, a three-way visibility system and, for the sanitizers,
generator expressions (`$<$<CONFIG:Debug>:...>`). Meson needs two objects per
library: the library itself and a `declare_dependency` that carries its include
path and link to users. The Makefile must list the libraries in a valid static
link order. Getting that wrong is a classic source of confusing errors. None of
these concepts is about C++.

== Adding a library <sec-change>

The most common structural change in a student project is extracting code into a
new library. We simulated it: a new library `score` (one header, one source file)
takes over the scoring table from `game`, and `game.cpp` includes
`"score/score.hpp"`. We then rebuilt with each tool *before* editing any build
file, as a student would.

#figure(
  table(
    columns: (auto, 1.6fr, auto, auto),
    align: (left, left, center, center),
    table.header[*Tool*][*Result before editing the build*][*Fix: lines added*][*changed*],
    [CMake], [link error: #raw("undefined reference to `blockfall::line_score(int, int)'")], [3], [1],
    [Meson], [same link error], [4], [2],
    [Make], [same link error], [2], [1],
    [#easypp], [builds; all tests pass], [0], [0],
  ),
  caption: [Extracting a new library without touching the build description.],
) <tab-change>

The failure is the same in all three tools, and it is the one that confuses
beginners most. The code compiles, because the header is found through an include
path the project already has, and then fails at link time with a message about an
undefined symbol in an object file. The student did nothing wrong in C++. They broke
a rule of a second language they may not know exists. The fix is small (three to
six lines) but needs the right words: the new library's name in the
`game` link line, in the correct position for Make. With #easypp, creating the
folder is the whole change. `easypp info` shows the new edge in the graph:
`game uses: bag, board, score`.

== What minimal set-up gives you <sec-defaults>

Most build tutorials start with the smallest possible build file. We built the
following program with the minimal set-up of each tool. That is a three-line
`CMakeLists.txt` (`cmake_minimum_required`, `project`, `add_executable`), a
two-line `meson.build`, and Make's built-in rule (`make main`, with `CXX=clang++`).

```cpp
int main(int argc, char*[]) {
    int total;                       // never initialised
    for (int i = 0; i < argc; ++i) total += i;
    int scores[argc];                // variable-length array: not standard C++
    scores[0] = total;
    std::printf("__cplusplus = %ld, score = %d\n", __cplusplus, scores[0]);
}
```

#figure(
  table(
    columns: (auto, auto, 1.3fr, auto),
    align: (left, center, left, center),
    table.header[*Set-up*][*Built?*][*Diagnostics*][*Standard used*],
    [CMake, 3 lines], [yes], [none], [C++17 + GNU extensions],
    [Make, built-in rule], [yes], [none], [C++17 + GNU extensions],
    [Meson, 2 lines], [yes], [2 warnings (`-Wuninitialized`, `-Wvla-cxx-extension`)], [C++17 + GNU extensions],
    [#easypp, file in `src/bin/`], [*no*], [2 errors, same checks], [C++23],
  ),
  caption: [Minimal projects compiling a program with two beginner mistakes. The standard
  is read from `__cplusplus` (201703 means C++17); without a flag, clang compiles
  `gnu++17` @clang16-notes.],
) <tab-minimal>

CMake and Make hand the compiler's defaults straight to the student. Those are no
warnings, and a C++ dialect two revisions behind the current standard, with GNU
extensions on, so the non-standard array compiles silently. Meson is better: its
default warning level shows both problems, but as warnings the program still
builds and runs. Each of these tools can be configured to behave like #easypp
(@sec-size shows how). The point is that the student, or the teacher on the
student's behalf, has to know to ask.

== Build time

#figure(
  table(
    columns: (auto, 1.6fr, auto, auto),
    align: (left, left, right, right),
    table.header[*Tool*][*Commands from a clean checkout to tested*][*Clean (s)*][*No-op (s)*],
    [CMake], [`cmake -S . -B build -G Ninja`, `cmake --build build`, `ctest --test-dir build`], [1.82], [0.025],
    [Meson], [`meson setup build`, `meson test -C build`], [2.34], [0.244],
    [Make], [`make -j16 all`, `make test`], [1.33], [0.025],
    [#easypp], [`easypp build`, `easypp test` (or just `easypp test`)], [2.18], [0.024],
  ),
  caption: [Clean configure + build + test, and a no-op rebuild; median of three runs.],
) <tab-time>

All four tools are fast enough for a project of this size; with sanitizers on,
every clean build takes 1.3–2.3 s. #easypp is slower than CMake and Make on the
first build, because that build also fills its cache (@sec-cache). Its no-op
rebuild is on par with CMake's and Make's. Without the cache (`--no-cache`), the
no-op rebuild takes about 0.10 s: the cost of asking clang for its search paths
and parsing about 1,900 pkg-config files on every run.

== An existing KDE application: KMines <sec-kmines>

Blockfall was written for this evaluation. To test an existing application's
requirements, we imported KDE's KMines at revision
`dbb447ca171469a55a5efbadbcbb480331dc7a38` into `examples/kmines`.
Its build exposed two missing capabilities: KDE library discovery without
pkg-config metadata and KConfig code generation. Both were added to #easypp;
there is no preparation script or replacement project build file.

The source adaptation moves the executable into `src/bin/kmines` and themes into
`data/kmines/themes`. Theme entries refer to the original SVG files instead of
compressed build outputs. Version and logging declarations become ordinary
source files; explicit header-moc includes are removed because #easypp compiles
those outputs separately. A deprecated spinbox in the UI is replaced by
`QSpinBox`; the form uses no plural suffix formatting.

#figure(
  table(
    columns: (1fr, auto, auto),
    align: (left, right, right),
    table.header[*Project build description*][*Physical lines*][*Non-blank, non-comment*],
    [Upstream: six CMake files], [258], [210],
    [Adapted KMines: build files], [0], [0],
    [Difference], [−258], [−210],
  ),
  caption: [KMines project build-file count at the pinned revision. Presets, CI and
    packaging files are excluded. Counts and reproduction instructions:
    `eval/kmines-lines.py` and `eval/kmines-lines.txt`.],
) <tab-kmines-size>

These lines move responsibility into the shared tool. The KDE implementation
adds 465 and deletes 13 physical lines across seven Rust files: a net increase
of 452, including unit tests. A further 41 lines add an end-to-end KConfig test.
These implementation counts include blanks and comments and exclude pre-existing
debugger edits; they use a different denominator from the effective build-file
count in @tab-kmines-size.

From a clean build directory, `easypp build` generates KConfig, moc, uic and rcc
outputs and links KMines with the normal debug sanitizers. `easypp run` launches
the game with its local themes. The KConfig regression test checks schema edits,
no-op builds, recovery when either generated output is deleted, and a generated
class requiring moc. An exported CMake project also builds.

This is a build-and-launch case study, not an installation-equivalence claim.
The upstream CMake files also handle installation, translations and
documentation. Those features are not reproduced here, and the source changes
above are part of the adaptation cost. The installed metadata reader supports
literal properties, not arbitrary CMake code or generator expressions. One
application demonstrates this path; it does not establish compatibility with
KDE software in general.

= Implications for teaching <sec-teaching>

*Teach the concepts, not the tool's syntax.* Sweller's cognitive-load account
@sweller1988 separates the load a task carries itself from load added by
how it is presented. Libraries, headers, translation units and linking are
material a C++ course has to teach. `target_link_libraries(... PUBLIC ...)` is
not; it is the build tool's own notation. #easypp keeps the concepts and drops
the notation. A library is still a library, but it is a folder, which the student
can see in the file browser. The link error of @tab-change disappears as a
_surprise_. It can come back as a _lesson_: `easypp build -v` prints the exact
compiler and linker commands, so a teacher can show what linking is once the
students are ready.

*The architecture is visible.* Because the layout is fixed, every project in the
course looks the same, and the folder tree shows the module structure. A cycle
between libraries stops the build with the cycle named. That teaches layering at
the moment it matters, not in a code review weeks later.

*Strict defaults are early feedback.* @tab-minimal is the typical first-week bug
class: uninitialised reads, non-standard extensions that work on one compiler
only. With `-Werror` these stop the build with a message that names the line.
With the defaults of CMake or Make they turn into wrong output at run time, or into
code that does not compile on the examiner's machine. Pinning C++23 also removes
a source of confusion seen in practice, courses teaching C++98-era code
@canabrava2026. The dialect is the same on every student's machine.

*Memory errors are caught where they happen.* With sanitizers on by default, an
out-of-bounds access, a use after free, a leak or a signed overflow stops the
program with a report naming the source line, instead of a crash somewhere else
or, worse, a run that seems correct. `easypp debug` stops the debugger right
there. These are the bugs a first C++ course spends most of its office hours on.

*Errors speak the course's language.* The link error of @tab-change is the kind
students cannot decode. #easypp's hints translate the common ones into the terms
of the project: this function is declared in this header but no `.cpp` defines
it; this definition does not match its declaration (@sec-help).

*Industry editors work out of the box.* The editors that dominate industry use
(VS Code 33%, CLion 23%, Visual Studio 21% @jetbrains2025) understand
`compile_commands.json`. #easypp always writes it, so clangd's completion,
navigation and the same diagnostics appear in the editor without set-up. A course
can therefore move off a legacy IDE without adding a build-system lesson.

*Tests are a folder.* A test is a program in `src/tests` that exits with 0. This
needs no framework and no registration step, so testing can be introduced in the
first weeks. Blockfall's four test programs check its rules without linking SDL,
which also shows students why separating logic from presentation pays off.

*A path to industry tools.* The layout maps one-to-one onto CMake concepts: a
folder in `src/libs` is an `add_library`, a discovered edge is a
`target_link_libraries`. A later course can introduce CMake by generating the
build file of a project the students already know with `easypp export-cmake`,
and then read it line by line: every line corresponds to a folder or an include
they wrote themselves.

= Limitations and threats to validity <sec-limits>

*Scope of the tool.* #easypp only targets Linux and clang. That is the most
serious limitation for teaching, since many students use Windows. Code::Blocks, for instance, is
distributed for Windows with a bundled MinGW compiler @codeblocks. It does not download packages:
system libraries must be installed, and third-party code must be copied into
`src/thirdparty`. It has no per-target flags, code generation other than Qt's
(moc, uic, rcc) and KDE KConfig, install step, shared libraries or C++20 modules. Modules would break the include-based discovery. These are not
oversights: each one would add a configuration surface. But they put a clear limit
on where the tool can be used.

*Heuristics.* Mapping a system header to a library is a guess. When several
versions are installed, the latest always wins, and there is deliberately no
way to ask for an older one. A header named like an unrelated library adds a
useless (though harmless, thanks to `--as-needed`) link flag. The include scan
evaluates only conditions it can decide and cannot follow macro includes. The cache's fingerprint uses folder modification
times, so a header added inside an existing sub-folder, without any library
folder changing, is not noticed until the next `easypp clean`.

*Measurements.* The controlled comparison uses Blockfall; KMines is an additional
build-and-launch case study, with source adaptation and a narrower scope than
its upstream installation build. The comparative build files were written
by the author of the tool being evaluated. We tried to write the shortest
idiomatic version for each tool and publish them so others can improve them. Line
counts measure the size of what must be written, not how hard it is to learn. The
timings are single-machine medians of three runs.

*No learning study.* This paper measures configuration burden, error behaviour
and defaults. It does not measure learning outcomes. The claim that removing build
notation improves learning rests on cognitive-load theory @sweller1988 and on the
failure modes we show. Testing it requires a controlled study in a course: the
same assignments, one group with CMake and one with #easypp, measuring time to the
first working multi-library program and the kinds of errors students meet. That
study is future work.

= Conclusion <sec-conclusion>

In C++, every small project currently records its structure twice, once in the
code and once in a build language. #easypp shows that for the projects students
write, the second copy can be derived from the first: a fixed folder layout plus
the `#include` lines say everything needed to build, link and test. On a 12-target
game, the tool replaces 25–38 lines of build code with none. It turns a link error
into a successful build when a library is added. It rejects two classic beginner
bugs that the minimal set-ups of CMake and Make let through, catches memory errors
at run time, explains the errors students meet most, and does so in comparable
build time. When students are ready for CMake, it writes their first
`CMakeLists.txt`. The other tools remain the right choice for projects that
outgrow a fixed layout. For the first contact with multi-file C++, sane defaults
and no build file let a course spend its time on C++.

#v(1em)
*Availability.* #easypp, its end-to-end tests, the examples (Blockfall with SDL2,
Life with GTK 4 and OpenMP, a Boost.Asio server, a Qt 6 application, and the adapted KDE KMines), the build
files for the other tools and the evaluation script are in the same repository
as this paper (`src/`, `tests/`, `examples/`, `paper/eval/`).

#bibliography("refs.yml", style: "ieee")
