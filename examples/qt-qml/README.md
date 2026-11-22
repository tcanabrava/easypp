# Qt Quick with a C++ backend

This example registers a C++ `Counter` type with `qmlRegisterType` and loads a
Qt Quick window from `qrc:/qml/Main.qml`. Tap the window to increment the
counter. easypp runs both moc and rcc and bundles the local QML component and
JavaScript helper.

Install the Qt 6 base and declarative development packages and the Qt Quick
runtime modules (`qt6-base` and `qt6-declarative` on Arch Linux). From this
directory, run `easypp run`. To check QML loading and the C++ property binding
without a display, run:

```sh
QT_QPA_PLATFORM=offscreen easypp run --release -- --check
```

The bundle is loaded by the Qt QML engine at runtime. C++ types are registered
in code, and named QML import modules supplied by Qt or other packages must be
installed where the engine can find them.
