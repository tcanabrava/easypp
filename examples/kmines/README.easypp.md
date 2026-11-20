# KMines with easypp

The real KDE KMines source, pinned to the commit in `UPSTREAM`, with its original
build files removed. Build easypp at the repository root, then:

```sh
cd examples/kmines
../../target/debug/easypp build
../../target/debug/easypp run
```

No preparation script, wrapper, project build file, or environment overrides are
needed. Requires Clang, pkg-config, Qt 6 development packages, KDE Frameworks 6,
libkdegames, and the KF6 KConfig compiler.

Easypp discovers KDE libraries through installed imported-target metadata and
follows their public dependencies, including Qt modules. It discovers
`settings.kcfgc`, reads its `File=kmines.kcfg`, and generates settings sources
under `build/<profile>/gen/bin/kmines` before moc and compilation. Both generated
settings outputs and the schema participate in incremental builds.

Source adaptations:

- Sources moved into `src/bin/kmines`.
- Version and logging declarations are ordinary source files, replacing the
  declarations formerly supplied by ECM's CMake macros.
- Explicit `moc_*.cpp` includes removed because easypp compiles header moc outputs
  separately. The source-level `mainwindow.moc` include remains.
- The UI uses QSpinBox instead of the deprecated KPluralHandlingSpinBox. Its
  upstream form uses no plural suffix formatting, so a standard spinbox suffices.
- Themes moved to `data/kmines/themes`; desktop theme entries refer directly to
  `.svg` files, avoiding build-time compression. Easypp run/debug prepends the
  project's `data` directory to XDG_DATA_DIRS.

The KDE metadata reader supports literal installed target properties, not the
full CMake language or generator expressions. It reads system package exports;
it does not invoke CMake or inspect a project's deleted build files. Installation
and translation compilation are outside this example's current scope.
