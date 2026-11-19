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
- **Include paths**: `src/libs` and each library folder are on the include path,
  so both `"static_lib1/lib1_header.hpp"` and `"lib1_header.hpp"` work. Each
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
     `/usr/lib64`, ...: `<pthread.h>` → `-lpthread`. Libraries outside the
     linker's default folders get `-L` and an rpath.
  4. `<math.h>` → `-lm`.

  Header-only libraries (`<nlohmann/json.hpp>`) link nothing. The flags come
  from `pkg-config`, which also follows `Requires:`. Without `pkg-config` they
  are read from the `.pc` file. Package `-I` flags become `-isystem`. Each
  library or executable gets only the flags of the system libraries it uses,
  directly or through the internal libraries it uses. System libraries are linked with
  `-Wl,--as-needed`, so an unnecessary guess is dropped by the linker.
  `easypp info` shows what was picked.
- **Compilers**: `.c` files use `clang -std=c17`; `.cpp`/`.cc`/`.cxx` files use
  `clang++ -std=c++20`. Executables that contain any C++ are linked with clang++.
  The flags are always `-Wall -pedantic -Werror`.
- **Builds are incremental and parallel**: clang depfiles track headers,
  and anything whose flags changed is rebuilt.
  `build/compile_commands.json` is written for clangd, VS Code and CLion.

## Building easypp

```sh
cargo install --path .
```
