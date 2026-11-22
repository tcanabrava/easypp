# Qt Quick with a named QML module

Install Qt 6.5 or later with its base and declarative development tools and
Qt Quick runtime modules. Build and run from this directory with `easypp run`.
For a headless runtime check, use:

```sh
QT_QPA_PLATFORM=offscreen easypp run --release -- --check
easypp lint --release
```

The executable's `qml/qmldir` declares `Easypp.Counter` and lists its public QML
components. easypp bundles the module under `qrc:/qt/qml/Easypp/Counter/`. The
C++ launcher loads `Main` by module URI. The module exposes the `Counter`
class through `QML_ELEMENT` in `Counter.hpp`.

easypp runs moc and Qt's `qmltyperegistrar` to produce the C++ registration and
`easypp.qmltypes` metadata. Named modules currently belong to individual
executables and use version 1.0. Put registered C++ types in headers. The
existing resource bundling and manual registration also work for apps that
load QML directly by URL.
