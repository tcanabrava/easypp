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
easypp test               # build and run every program in src/tests
easypp build --release    # -O2 -DNDEBUG instead of -O0 -g
easypp info               # what was discovered, and what links to what
easypp clean
```

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
  `-isystem`, so warnings from third-party code don't break the build. The
  system's default include and library folders are used as usual.
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

  Generated files go to `build/<profile>/gen/`, which is on the include path of
  the component and of everything that uses it. The tools are found through
  pkg-config (`/usr/lib/qt6/moc` and so on), not PATH.
- **Compilers**: `.c` files use `clang -std=c17`; `.cpp`/`.cc`/`.cxx` files use
  `clang++ -std=c++23`. Executables that contain any C++ are linked with clang++.
  The flags are always `-Wall -pedantic -Werror`.
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

## Building easypp

```sh
cargo install --path .
```
