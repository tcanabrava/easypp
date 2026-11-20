// A Qt Widgets to-do list.
//
//   tasks                                  open the window
//   tasks --smoke 2 --screenshot shot.png  add sample tasks, save the window, quit

#include "window/mainwindow.hpp"

#include <QApplication>
#include <QTimer>

#include <charconv>
#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace {

// A whole command-line number, or false if the text is not one (unlike atof,
// which silently turns "abc" into 0).
template <typename T> bool parse_number(const char* text, T& out)
{
    const char* end = text + std::strlen(text);
    const auto [last, error] = std::from_chars(text, end, out);
    return error == std::errc() && last == end;
}

}

int main(int argc, char* argv[])
{
    double smoke = 0;
    const char* screenshot = nullptr;
    for (int i = 1; i + 1 < argc; ++i) {
        if (std::strcmp(argv[i], "--smoke") == 0) {
            if (!parse_number(argv[++i], smoke)) {
                std::fprintf(stderr, "--smoke needs a number of seconds\n");
                return 2;
            }
        } else if (std::strcmp(argv[i], "--screenshot") == 0) {
            screenshot = argv[++i];
        }
    }

    QApplication app(argc, argv);
    TaskList tasks;
    MainWindow window(tasks);
    window.show();

    if (smoke > 0) {
        tasks.add("Write the build file");
        tasks.add("Run moc, uic and rcc by hand");
        tasks.add("Let easypp do it all");
        tasks.setDone(0, true);
        tasks.setDone(1, true);
        QTimer::singleShot(static_cast<int>(smoke * 1000), &app, [&] {
            std::printf("smoke test: %d tasks, %d left, about text %s\n", tasks.count(), tasks.remaining(),
                MainWindow::aboutText().isEmpty() ? "MISSING" : "loaded from resources");
            if (screenshot != nullptr) {
                std::printf("%s %s\n", window.grab().save(screenshot) ? "saved" : "could not save", screenshot);
            }
            app.quit();
        });
    }
    return app.exec();
}
