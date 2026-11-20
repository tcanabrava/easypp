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
  #text(16pt, fill: muted)[From a multi-file program to a real KDE application]
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

#slide[The first working project][
  ```sh
  easypp new hello
  cd hello
  easypp run
  easypp test
  ```
  #v(0.4em)
  #cols([
    `new` creates a library, a program, a test, and editor formatting/lint settings.
  ], [
    `build` writes `compile_commands.json` and compiles in parallel with Clang.
  ])
  #v(0.7em)
  #note[The same commands work after the project grows to several libraries.]
]

#slide[A Qt application is just a source file][
  #cols([
    #set text(size: 17pt)
    ```cpp
    // src/bin/window.cpp
    #include <QApplication>
    #include <QPushButton>

    int main(int argc, char **argv)
    {
        QApplication app(argc, argv);
        QPushButton button("Hello");
        button.show();
        return app.exec();
    }
    ```
  ], [
    #label[Build and launch]
    ```sh
    easypp run window
    ```
    #text(18pt)[The headers select Qt 6 Widgets. Installed metadata supplies its include paths and dependencies.]
    #v(0.6em)
    #text(18pt)[No Qt module list or linker flags in a project file.]
  ])
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

#slide[System libraries come from installed metadata][
  #table(columns: (1fr, 1fr),
    [Source], [Discovered dependency],
    [`<SDL2/SDL.h>`], [`sdl2` through pkg-config],
    [`<boost/json.hpp>`], [`libboost_json`],
    [`<QPushButton>`], [Qt 6 Widgets + its dependencies],
    [`<KXmlGuiWindow>`], [Installed KDE target metadata],
    [`<omp.h>` or an OpenMP pragma], [`-fopenmp`],
  )
  #v(0.5em)
  #text(17pt)[Header ownership, package include paths and library names provide the clues.
  System headers use `-isystem`; system libraries use `--as-needed`.]
  #v(0.3em)
  #text(17pt, fill: muted)[Dependencies must already be installed. easypp does not download packages.]
]

#slide[Code generation follows the same rule][
  #table(columns: (1fr, 1.3fr, 1fr),
    [Discovered input], [Tool], [Build output],
    [`Q_OBJECT` in a header/source], [Qt moc], [Meta-object code],
    [`form.ui`], [Qt uic], [`ui_form.h`],
    [`resources.qrc`], [Qt rcc], [Compiled resources],
    [`settings.kcfgc` + its schema], [KConfig compiler, then moc], [Settings header/source],
  )
  #v(0.5em)
  Generated files live under `build/<profile>/gen/`.\
  Generation runs before compilation and tracks its inputs.
  #v(0.4em)
  #text(17pt, fill: muted)[Qt resources from static libraries link directly into consumers so the linker retains them.]
]

#slide[Strict defaults are part of the interface][
  #cols([
    #label[Every own-source build]
    ```text
    clang / clang++
    -std=c17 / -std=c++23
    -Wall -pedantic -Werror
    ```
  ], [
    #label[Debug is the default]
    ```text
    -O0 -g
    AddressSanitizer
    UndefinedBehaviorSanitizer
    ```
  ])
  #v(0.5em)
  `--release` selects `-O2 -DNDEBUG`.\
  Third-party sources compile separately with warnings disabled.
  #v(0.3em)
  #text(17pt, fill: muted)[Fixed policy reduces configuration choices; it also limits where this tool fits.]
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

#slide[Keep the compiler error; explain the project][
  #cols([
    #label[What easypp checks]
    - Wrong header ownership/spelling
    - Declaration without a definition
    - Signature mismatch
    - Missing function body or `main()`
    - A library header never included
  ], [
    #label[Make the investigation visible]
    ```sh
    easypp info
    easypp build -v
    easypp debug play
    ```
    Debugging uses the installed debugger. Sanitizer failures stop at the fault.
  ])
  #v(0.35em)
  #text(17pt, fill: muted)[Hints supplement compiler and linker diagnostics; they do not replace them.]
]

#slide[Tests and editor support are built in][
  #cols([
    #label[Tests are a folder]
    - Programs under `src/tests`
    - Parallel execution
    - 60-second default time limit
    - Only failing tests print output
    - GoogleTest/Catch2 main detection
  ], [
    #label[Use ordinary C++ editors]
    - `compile_commands.json` for clangd
    - `watch` updates discovery/codegen
    - `fmt` runs clang-format
    - `lint` runs clang-tidy
    - Project formatting settings win
  ])
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
  #source[Clang 23.1.5, CMake 4.5.0, Meson 1.12.1, Ninja 1.13.2, Make 4.4.1. paper/eval/results.txt.]
]

#slide[Then try an application we did not write][
  #label[KDE KMines]
  #v(0.4em)
  - Import upstream sources and remove project build files.
  - Move the executable into `src/bin/kmines`.
  - Build and launch with ordinary easypp commands.
  #v(0.7em)
  #note[The attempt exposed missing support for KDE library metadata and KConfig generation. We added both to the shared tool.]
  #source[Pinned KMines revision dbb447ca171469a55a5efbadbcbb480331dc7a38; examples/kmines/UPSTREAM.]
]

#slide[What the KDE support actually does][
  #cols([
    #label[Installed library metadata]
    - Index literal imported-target properties.
    - Follow include paths, definitions and public dependencies.
    - Resolve Qt dependencies through installed pkg-config metadata.
  ], [
    #label[KConfig generation]
    - Discover `settings.kcfgc`.
    - Resolve its `File=` schema.
    - Generate `.h` and `.cpp` before moc.
    - Regenerate after schema edits or deletion of either output.
  ])
  #v(0.45em)
  #text(18pt, weight: "bold", fill: blue)[No project preparation script or dependency list.]
  #source[src/kde.rs, src/qt.rs; integration test: kde_kconfig_incremental_build.]
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

#slide[The adaptation still has a cost][
  #cols([
    #label[Source and layout changes]
    - Move executable and theme folders.
    - Supply ordinary version/logging declarations.
    - Remove separately compiled moc includes.
    - Replace a deprecated spinbox.
    - Refer directly to SVG themes.
  ], [
    #label[What we verified]
    - Clean debug build with sanitizers
    - Launch with local runtime assets
    - KConfig classes requiring moc
    - Incremental regeneration/recovery
    - Build of exported CMake project
  ])
  #v(0.4em)
  #text(17pt, fill: muted)[One successful application is a case study, not proof of general KDE compatibility.]
]

#slide[The boundary is deliberate][
  #cols([
    #label[Current scope]
    - Linux and Clang
    - Installed system dependencies
    - Fixed flags and folder layout
    - Static project libraries
    - Qt 6 and KConfig code generation
  ], [
    #label[Outside that scope]
    - Package downloading
    - Per-target build policy
    - Shared project libraries / C++ modules
    - Installation and packaging
    - Arbitrary CMake expressions
  ])
  #v(0.4em)
  #text(17pt, fill: muted)[Include scanning is not a full preprocessor. Unknown conditions are scanned conservatively;
  macro includes cannot be followed. Header-to-library mapping remains heuristic.]
]

#slide[A path to the next course][
  ```sh
  easypp export-cmake
  ```
  #cols([
    #label[Concepts already learned]
    #v(0.3em)
    Folder = library or executable\
    Include = dependency\
    Program exit status = test result
  ], [
    #label[Their CMake names]
    #v(0.3em)
    `add_library` / `add_executable`\
    `target_link_libraries`\
    `add_test`
  ])
  #v(0.65em)
  #note[Generate the build description of a project the students already understand, then teach it.]
]

#slide[What this evidence supports][
  - Folder conventions can remove project build-file maintenance for this scope.
  - Strict defaults give earlier feedback on common beginner mistakes.
  - Shared framework support can carry an existing KDE game through build and launch.
  #v(0.6em)
  #text(21pt, weight: "bold", fill: blue)[Learning outcomes still need a classroom study.]
  #v(0.4em)
  #text(18pt)[The controlled benchmark uses one author-written game. KMines adds an existing application,
  with source adaptation and a narrower scope than its upstream build.]
]

#page(footer: none)[
  #v(1.2cm)
  #text(39pt, weight: "bold")[Teach libraries, headers and linking.]
  #v(0.45em)
  #text(32pt, fill: blue)[Let conventions carry the build.]
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
