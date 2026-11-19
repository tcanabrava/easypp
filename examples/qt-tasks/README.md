# qt-tasks: a Qt Widgets example for easypp

A to-do list that uses each part of Qt's build tooling, with no build file:

| File | Tool | What easypp does |
|---|---|---|
| `src/libs/tasks/tasklist.hpp` (`Q_OBJECT`, signals, `Q_PROPERTY`) | moc | `moc_tasklist.cpp`, compiled into `libtasks.a` |
| `src/libs/window/mainwindow.hpp` (`Q_OBJECT`) | moc | `moc_mainwindow.cpp`, compiled into `libwindow.a` |
| `src/libs/window/mainwindow.ui` | uic | `ui_mainwindow.h`, on `window`'s include path |
| `src/libs/window/resources.qrc` (icon, About text) | rcc | `qrc_resources.cpp`, linked into every program using `window` |
| `src/tests/*_test.cpp` (`Q_OBJECT` in the `.cpp`) | moc | `<name>.moc`, included at the end of the test |

The `tasks` library uses only Qt Core. `window` uses Qt Widgets, and the tests
use Qt Test. easypp links exactly those modules (`easypp info` shows them).

```sh
easypp run tasks                                         # open the window
easypp run tasks -- --smoke 2 --screenshot shot.png      # sample tasks, save, quit
easypp test                                              # Qt Test, offscreen
```
