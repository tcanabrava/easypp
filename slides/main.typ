// easypp conference presentation, based on ../paper/main.typ.
// Build from the repository root: typst compile slides/main.typ slides/easypp.pdf
#set document(title: "Folders, Not Build Files — easypp", author: "Tomaz Canabrava")
#let ink = rgb("#172330")
#let muted = rgb("#566574")
#let blue = rgb("#2469ba")
#let pale = rgb("#eef4fb")
#let green = rgb("#187653")
#set page(paper: "presentation-16-9", margin: (x: 1.5cm, top: 1.15cm, bottom: 1.3cm), fill: rgb("#fcfcfb"), footer: context {
  set text(size: 10pt, fill: muted)
  grid(columns: (1fr, auto), [easypp · Folders, Not Build Files], counter(page).display())
})
#set text(font: "Noto Sans", size: 20pt, fill: ink)
#set par(leading: 0.55em)
#set list(spacing: 0.65em, marker: text(fill: blue)[•])
#show raw: set text(font: "DejaVu Sans Mono", size: 16pt)
#show raw.where(block: true): block.with(fill: pale, inset: 12pt, radius: 5pt, width: 100%)
#set table(inset: (x: 9pt, y: 8pt), stroke: (bottom: 0.6pt + rgb("#d3dce6")))
#show table: set text(size: 18pt)
#show table.cell.where(y: 0): set text(weight: "bold", fill: blue)
#let slide(title, body, source: none) = page[
  #text(30pt, weight: "bold", title)
  #v(0.65em)
  #body
  #if source != none { place(bottom + left, text(10pt, fill: muted, source)) }
]
#let source(body) = place(bottom + left, text(10pt, fill: muted, body))
#let cols(a, b) = grid(columns: (1fr, 1fr), column-gutter: 1cm, a, b)
#let note(body) = block(fill: pale, inset: 12pt, radius: 4pt, width: 100%)[#body]
#let label(body) = text(21pt, weight: "bold", fill: blue, body)
#let tile(number, body) = block(stroke: (left: 4pt + blue), inset: (left: 15pt, y: 8pt))[
  #text(42pt, weight: "bold", fill: blue, number) \
  #text(18pt, body)
]

#page(footer: none)[
  #v(1.1cm)
  #text(19pt, weight: "bold", fill: blue)[easypp]
  #v(0.4em)
  #text(45pt, weight: "bold")[Folders, Not Build Files]
  #v(0.35em)
  #text(25pt, fill: muted)[A zero-configuration C/C++ build tool for teaching]
  #v(1.1cm)
  #text(21pt)[Tomaz Canabrava]
  #v(0.5em)
  #text(16pt, fill: muted)[A C++ workflow from a first library to a desktop application]
]

#slide[Extract a library. Meet the linker.][
  #cols([
    #label[The C++ change]
    ```cpp
    // game.cpp
    #include "score/score.hpp"

    total += line_score(lines);
    ```
    A declaration is visible.\
    The implementation exists.
  ], [
    #label[The build after that change]
    ```text
    undefined reference to
      line_score(int)
    ```
    The new library also needs a target, dependencies and a place on the link line.
  ])
  #v(0.6em)
  #note[The source records a dependency. The build description records it again.]
]

#slide[What must a beginner learn first?][
  #cols([
    #label[C++ concepts]
    - Headers and declarations
    - Translation units
    - Libraries and linking
    - Tests and memory errors
  ], [
    #label[Build-language concepts]
    - Targets and source lists
    - Include-path propagation
    - `PUBLIC`, `PRIVATE`, `INTERFACE`
    - Generators and configuration flags
  ])
  #v(0.6em)
  #note[Can we teach the C++ concepts before requiring a second language?]
]

#slide[The proposal: a fixed layout is enough][
  #cols([
    ```text
    project/
      src/
        libs/board/
        libs/game/
        bin/play.cpp
        tests/rules.cpp
        thirdparty/vendor/
      data/<application>/
    ```
  ], [
    - A library folder becomes a static library.
    - A binary file or folder becomes an executable.
    - A test is a program that exits with 0.
    - Includes determine which libraries are linked.
  ])
  #v(0.35em)
  #text(22pt, weight: "bold", fill: blue)[There is no project build file.]
]

#slide[The first success arrives early][
  #text(27pt)[A student starts with a program, a library and one test.]
  #v(0.6em)
  #cols([
    #label[First run]
    The executable makes the result something the student can see.
  ], [
    #label[First change]
    A test gives the student a way to tell whether that change worked.
  ])
  #v(0.7em)
  #note[That same workflow is still there after the source tree grows.]
]

#slide[What changes when Hello World gets a window?][
  #cols([
    #label[The source]
    ```cpp
    #include <QApplication>
    #include <QPushButton>
    ```
    The new code says it needs a button and an application.
  ], [
    #label[The build]
    The installed Qt metadata tells easypp which library owns those headers and what that library needs.
  ])
  #v(0.7em)
  #note[The student can move from a console exercise to a desktop app using the layout they already know.]
]

#slide[An include creates a dependency][
  #cols([
    ```cpp
    // src/bin/play.cpp
    #include "game/game.hpp"

    // src/libs/game/game.hpp
    #include "board/board.hpp"
    ```
  ], [
    #label[Discovered graph]
    #v(0.4em)
    #text(25pt)[play → game → board]
    #v(0.65em)
    Link dependents before dependencies:
    ```text
    play.o libgame.a libboard.a
    ```
  ])
  #v(0.35em)
  A cycle produces an error naming the cycle.\
  `easypp info` exposes the graph; `build -v` shows the commands.
]

#slide[Header spelling makes ownership visible][
  #cols([
    #label[Use the library's name]
    ```cpp
    #include "board/board.hpp"
    ```
    Only `src/libs` is on the project include path. Two libraries can have a `utils.hpp`.
  ], [
    #label[Inside the same library]
    ```cpp
    #include "board.hpp"
    ```
    Relative includes work next to the including source. Bare includes elsewhere get a hint.
  ])
  #v(0.65em)
  #note[The include identifies both the header and the library that supplies it.]
]

#slide[The same include can mean different things][
  #cols([
    #label[Inside the project]
    `"game/game.hpp"` points to a library the student can open and edit.
  ], [
    #label[On the machine]
    `<QPushButton>` points to a library installed with Qt.
  ])
  #v(0.85em)
  #note[This is where inference has a limit. Competing installed packages can provide the same header, so easypp shows which one it chose.]
]

#slide[The assignment has to work on a second laptop][
  #text(25pt)[An include tells us which API the student wants. It cannot tell us which version the assignment used.]
  #v(0.7em)
  #cols([
    #label[The source tree]
    Determines who uses zlib, JSON and GoogleTest.
  ], [
    #label[The Conan manifest]
    Records the versions a fresh checkout should restore.
  ])
  #v(0.7em)
  #source[examples/conan-json; verified selected headers and libraries with compiler depfiles and linker tracing.]
]

#slide[An import changes the compilation order][
  #label[The example crosses two static libraries]
  #v(0.45em)
  #grid(
    columns: (1fr, auto, 1fr, auto, 1fr), column-gutter: 0.35cm,
    align: horizon,
    note[`arithmetic`#linebreak()#text(16pt)[interface and partition]],
    [→],
    note[`report`#linebreak()#text(16pt)[imports arithmetic]],
    [→],
    note[`demo.cpp`#linebreak()#text(16pt)[imports report]],
  )
  #v(0.75em)
  - Providers compile first and emit a compiled interface with their object.
  - Imports also determine which static libraries the executable links.
  - Interface edits and Clang changes invalidate cached module builds.
  #source[examples/modules. Arrows show build order. Import relationships run in the opposite direction.]
]

#slide[Three ways to import in C++][
  #table(columns: (1fr, 1.3fr, 1.55fr),
    [Syntax], [Where it comes from], [How easypp handles it],
    [`import report;`], [A project source], [Clang builds it before consumers.],
    [`import <array>;`], [A header], [Clang precompiles it as a header unit.],
    [`import std;`], [The toolchain], [Easypp locates the source through the installed module manifest.],
  )
  #v(0.45em)
  #text(18pt)[For named-module projects, `clang-scan-deps` can find imports in
  included headers and follow active preprocessor branches.]
  #v(0.25em)
  #text(16pt, fill: muted)[CMake export covers named modules, including `std` with CMake 4.5 and Ninja.
  Header units build directly with easypp.]
  #source[examples/modules, examples/header-units, examples/std-module. Demonstrations, separate from Blockfall timings.]
]

#slide[A framework can bring its own source language][
  #text(25pt)[A Qt Quick view can instantiate a class written in C++.]
  #v(0.6em)
  #cols([
    #label[C++]
    ```cpp
    class Counter : public QObject {
        Q_OBJECT
        QML_ELEMENT
        // ...
    };
    ```
  ], [
    #label[QML]
    ```qml
    import Easypp.Counter

    Counter { id: counter }
    ```
  ])
  #note[#text(18pt)[Qt generates the registration. The same type information lets lint catch a misspelled property before launch.]]
  #source[examples/qt-qml-module; native build, CMake export and QML lint verified.]
]

#slide[What feedback does a student get by default?][
  #cols([
    #label[At compilation]
    Strict warnings stop the build when the compiler sees a likely mistake.
  ], [
    #label[At execution]
    Debug sanitizers can stop a program at an invalid memory access and point back to the source.
  ])
  #v(0.7em)
  #note[Students can use both before they learn to configure warning flags or sanitizers.]
]

#slide[Minimal setup changes the feedback][
  #text(18pt)[The experiment compiles an uninitialised read and a variable-length array.]
  #v(0.4em)
  #table(columns: (1fr, 1fr, 1fr),
    [Minimal project], [Diagnostics], [Compiler dialect],
    [CMake: 3 lines], [None; builds], [GNU C++17],
    [Make: built-in rule], [None; builds], [GNU C++17],
    [Meson: 2 lines], [Two warnings; builds], [GNU C++17],
    [easypp: file in `src/bin`], [Two errors; stops], [C++23],
  )
  #v(0.5em)
  #text(18pt)[All these tools can enforce strict settings. Here, the student gets them before learning how to configure them.]
  #source[Measured with Clang 23.1.5; paper/eval/results.txt. Compiler defaults are environment-specific.]
]

#slide[An error should tell you where to look][
  #cols([
    #label[Before a build]
    Doctor checks which tools and development files the project needs.
  ], [
    #label[After a failure]
    The compiler's diagnostic remains visible. easypp can point from a missing symbol back to the project's declarations and definitions.
  ])
  #v(0.7em)
  #note[A student can start by asking whether the environment is ready, then investigate the code when it is.]
]

#slide[Testing becomes part of the edit][
  #text(26pt)[A test is another C++ program with an answer to check.]
  #v(0.7em)
  #cols([
    Put it with the other tests and it runs alongside them.
  ], [
    The editor reads the same compilation flags as the build, so it sees the same headers.
  ])
  #v(0.7em)
  #source[Tests, compile_commands.json and watch are described in the paper.]
]

#slide[Evaluate on a layered game: Blockfall][
  #grid(columns: (1fr, 1fr, 1fr), column-gutter: 0.7cm,
    tile[7][internal libraries\ (2 header-only)],
    tile[1][executable\ with SDL2],
    tile[4][test programs],
  )
  #v(0.8em)
  #text(23pt)[render → game → {bag, board} → pieces]
  #v(0.6em)
  #text(18pt)[We wrote CMake, Meson and Make descriptions with matching language,
  warning, sanitizer and test settings. Compare build-code size, a structural change,
  defaults and build time.]
  #source[Blockfall was written for this evaluation. Build descriptions were written by the tool's author.]
]

#slide[How much build code must be written?][
  #cols([
    #table(columns: (1fr, auto, auto),
      [Tool], [Lines], [Words],
      [CMake], [38], [102],
      [Meson], [25], [124],
      [Make], [35], [171],
      [*easypp*], [*0*], [*0*],
    )
  ], [
    #label[The cost is also vocabulary]
    #v(0.3em)
    The 38-line CMake description uses 17 commands, target visibility and generator expressions.
    #v(0.6em)
    easypp moves that policy into the tool and fixes the project layout.
  ])
  #source[Blockfall; non-blank, non-comment lines. paper/eval/results.txt; reproduction: paper/eval/run.sh.]
]

#slide[Extract scoring into a new library][
  #text(19pt)[Add `src/libs/score/` and include `"score/score.hpp"` from the game. Rebuild before editing any build description.]
  #v(0.5em)
  #table(columns: (1fr, 1.5fr, auto, auto),
    [Tool], [First rebuild], [Lines added], [Changed],
    [CMake], [Undefined reference], [3], [1],
    [Meson], [Undefined reference], [4], [2],
    [Make], [Undefined reference], [2], [1],
    [*easypp*], [*Builds; tests pass*], [*0*], [*0*],
  )
  #v(0.5em)
  #note[The folder adds the library. The include adds the edge.]
  #source[Blockfall extraction experiment; paper/eval/results.txt. Counts describe the subsequent build-file fix.]
]

#slide[Build performance is in the same range][
  #table(columns: (1fr, auto, auto),
    [Tool], [Clean build + test], [No-op rebuild],
    [CMake + Ninja], [1.82 s], [0.025 s],
    [Meson + Ninja], [2.34 s], [0.244 s],
    [Make], [1.33 s], [0.025 s],
    [easypp], [2.18 s], [0.024 s],
  )
  #v(0.55em)
  #text(19pt)[easypp's first build also fills its discovery cache.
  Later builds reuse system metadata and parsed include lists.]
  #v(0.35em)
  #text(17pt, fill: muted)[One Linux machine, 16 threads; median of three runs. These are not general build-system speed rankings.]
  #source[Blockfall snapshot, 20 Nov 2026. Clang 23.1.5, CMake 4.5.0, Meson 1.12.1, Ninja 1.13.2, Make 4.4.1.]
]

#slide[Could the convention handle somebody else's code?][
  #label[KDE KMines]
  #v(0.45em)
  #text(25pt)[KMines already knew how to build itself. Could the source tree and installed libraries supply enough information to build and launch it?]
  #v(0.75em)
  #note[The first attempt exposed two gaps in easypp: KDE's installed dependency metadata and generated settings classes.]
  #source[Pinned KMines revision in examples/kmines/UPSTREAM.]
]

#slide[The missing knowledge moved into the tool][
  #cols([
    #label[Finding KDE libraries]
    Installed metadata says which headers, libraries and other dependencies belong together.
  ], [
    #label[Generating settings]
    A KConfig schema gives Qt code a settings class. A schema edit must regenerate the class before compilation.
  ])
  #v(0.8em)
  #note[These two conventions now work for other projects, too.]
]

#slide[KMines: configuration removed, tool code added][
  #cols([
    #table(columns: (1fr, auto, auto),
      [Build files], [Physical], [Effective],
      [Six upstream CMake files], [258], [210],
      [Adapted KMines], [0], [0],
      [*Difference*], [*−258*], [*−210*],
    )
  ], [
    #label[Shared implementation cost]
    #v(0.4em)
    #text(29pt, weight: "bold")[+465 − 13 = +452]
    #v(0.3em)
    #text(18pt)[Physical lines across seven Rust files, including unit tests.]
    #v(0.4em)
    #text(18pt)[Integration test: *+41 lines*.]
  ])
  #v(0.6em)
  #text(17pt, fill: muted)[Effective excludes blanks and full-line comments. Upstream also handles installation,
  translations and documentation; this experiment verifies building and launching.]
  #source[Pinned upstream; reproducible counts: paper/eval/kmines-lines.py and kmines-lines.txt.]
]

#slide[What came along with the source?][
  #cols([
    #label[We kept]
    The game, its settings schema and its themes.
  ], [
    #label[We adapted]
    File placement, a deprecated widget and how the game loads its themes.
  ])
  #v(0.8em)
  #note[The result is a build-and-launch case study. The upstream project also handles installation, translations and documentation.]
]

#slide[The VTE demo brought its own UI with it][
  #text(25pt)[The upstream demo keeps its GtkBuilder forms, menus and CSS, and links against an installed VTE widget library.]
  #v(0.7em)
  The build has to follow those assets from their manifest to the executable. Editing a form must reach the next launch.
  #v(0.75em)
  #source[examples/vte; clean build, desktop launch and CMake export verified.]
]

#slide[One board. Three kinds of work.][
  #grid(columns: (1fr, 1fr, 1fr), column-gutter: 0.6cm,
    [#label[CUDA] #parbreak() Evolves the board.],
    [#label[GTK 4] #parbreak() Shows the result and handles input.],
    [#label[OpenMP] #parbreak() Counts live cells and computes a CPU reference.],
  )
  #v(0.85em)
  #note[On the RTX A500, the GPU and CPU boards matched cell for cell over 24 generations.]
  #v(0.45em)
  #text(17pt, fill: muted)[The CPU and GPU started from the same board and compared the result after each step.]
  #source[examples/life-cuda; headless comparison and GTK launch verified.]
]

#slide[Where does inference stop helping?][
  #cols([
    #label[What the source tells us]
    Who includes a header, which library owns it, and which modules must compile first.
  ], [
    #label[What still needs a decision]
    How to install a program, how to arrange shared libraries, and which external package variant a project intends to use.
  ])
  #v(0.8em)
  #text(18pt)[A compiler can see dependencies hidden behind macros. A scan of the source can miss them.]
]

#slide[Teach CMake with a project students already know][
  #text(25pt)[Start with the program they wrote and the library they extracted.]
  #v(0.65em)
  #cols([
    #label[The familiar project]
    A library folder, a program that uses it and a test.
  ], [
    #label[Its exported CMake]
    Those same parts now have names in a build language.
  ])
  #v(0.7em)
  #note[Students can read the generated description and connect each target to code they already understand.]
]

#slide[What would we test in a classroom?][
  #text(26pt)[Can students reach a working multi-library program sooner when they start with source conventions?]
  #v(0.7em)
  #cols([
    #label[What we measured]
    Build-file size, the cost of a library extraction, compiler feedback and build time.
  ], [
    #label[What a course study could measure]
    Time to a first working program, the errors students meet, and what they understand about linking.
  ])
]

#page(footer: none)[
  #v(1.2cm)
  #text(39pt, weight: "bold")[What should a student have to learn first?]
  #v(1cm)
  #text(22pt)[easypp · Folders, Not Build Files]
  #v(0.4em)
  #text(17pt)[Paper: `paper/main.typ`\
  Implementation: `src/` · Examples: `examples/`\
  Reproduce the measurements: `paper/eval/`]
  #v(0.6em)
  #text(20pt)[Questions]
]

#slide[Appendix: discovery and cache mechanics][
  #text(22pt, weight: "bold", fill: blue)[Source tree → include graph → dependencies → codegen → compile/link]
  #v(0.6em)
  - Clang reports its include and library search paths.
  - Package metadata maps system headers to compile/link flags.
  - A per-project Turso database caches metadata and parsed include lists.
  - Depfiles and command stamps drive incremental compilation.
  - `--no-cache` bypasses discovery caching; `clean` deletes build outputs.
  #v(0.4em)
  #text(17pt, fill: muted)[The cache fingerprint uses metadata timestamps; changes deep inside existing include folders
  can require a clean rebuild to refresh discovery.]
]

#slide[Appendix: reproduce the experiments][
  #set text(size: 18pt)
  ```sh
  cargo build --release
  paper/eval/run.sh "$PWD/target/release/easypp"
  cd examples/kmines
  ../../target/release/easypp build
  ../../target/release/easypp run
  ```
  #v(0.35em)
  #text(17pt)[KConfig regression test (requires KDE development tools):]
  ```sh
  cargo test kde_kconfig_incremental_build -- --ignored
  ```
  #text(16pt, fill: muted)[Run the test from the repository root. KMines count reproduction instructions are in
  paper/eval/kmines-lines.py. The benchmark commands require installed development packages.]
]

#slide[Appendix: sources and further reading][
  #set text(size: 18pt)
  - *Paper:* “Folders, Not Build Files: A Zero-Configuration C++ Build Tool for Teaching”, `paper/main.typ`.
  - *Controlled evaluation:* scripts, build descriptions and measured output in `paper/eval/`.
  - *KMines case study:* pinned revision in `examples/kmines/UPSTREAM`; source changes in `README.easypp.md`.
  - *Related tools:* Cargo conventions, CMake, Meson, Make, Bazel, xmake and build2; references in `paper/refs.yml`.
  - *Teaching motivation:* cognitive-load theory motivates the design; this work does not measure learning gains.
]
