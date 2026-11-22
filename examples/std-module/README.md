# Standard library modules

Run `easypp run --release` to print `42`, then `easypp test --release` to
exercise `std.compat`. No build files or prebuilt module files are required.

Clang and its selected standard library must provide compatible module sources
and a module manifest. Easypp discovers those files when it sees `import std;`
or `import std.compat;` and writes the compiled interfaces under `build/`.
Tested with Clang 23.1.5 and libstdc++ 16. Build this example directly with
easypp, since its CMake exporter does not configure standard library modules.
