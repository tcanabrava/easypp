# C++ named modules

Run `easypp run` in this directory to print `35`, then run `easypp test`.
The example uses the same folder layout as an include-based project.

- `arithmetic` exports a partition and declares a function defined in
  `triple.cpp`, a module implementation unit.
- `report` imports `arithmetic` from a different static library.
- The executable and test import `report`. Easypp discovers both libraries
  and links them in dependency order.

Easypp builds each compiled module interface before its importers.
`easypp export-cmake` emits module file sets for CMake 3.28 or newer.
For Clang, use Ninja and install the matching `clang-scan-deps` tool when
building the exported project.
