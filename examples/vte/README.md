# VTE's GTK 4 demo with easypp

This example imports the C++ GTK 4 demo application and its support files
from VTE 0.84.1. See [UPSTREAM](UPSTREAM) for the exact revision. It links against
the installed VTE widget library, so this experiment builds the upstream
application rather than rebuilding the widget library itself.

Install the GTK 4 and VTE GTK 4 development packages, GLib development tools
(including `glib-compile-resources`), fmt 12, PCRE2 headers, Clang and pkg-config.
Build easypp from the repository root, then run:

```sh
cd examples/vte
../../target/debug/easypp build
../../target/debug/easypp run
```

Easypp generates C from the upstream GResource manifest, tracks referenced
assets as dependencies, and discovers GTK 4 and VTE through pkg-config.
No Meson files or preparation scripts are needed. The import adds a small
source header for the demo's GTK 4 settings and adapts the upstream source
for strict C++23, modern fmt, GLib enum parsing and UBSan. Existing GTK
deprecation annotations are disabled in the demo's source configuration.
