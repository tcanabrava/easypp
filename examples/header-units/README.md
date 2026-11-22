# C++ header units

Run `easypp run` in this directory. The program exits successfully when the
imported `scale` function doubles the imported constant to `42`.

The source imports a header next to the executable, a header from another
library, and the standard library's `<array>`. Easypp compiles each imported
header into a Clang PCM and links the `scale` library through the same
discovery rules as an include. Changes to an imported header or its textual
includes trigger recompilation.

Header units use Clang's experimental support. Imports must be written
directly in C++ source files. Imported headers can use `#include`, but
easypp does not discover further `import` declarations inside them. Easypp's
CMake exporter reports an error for projects using header units.
