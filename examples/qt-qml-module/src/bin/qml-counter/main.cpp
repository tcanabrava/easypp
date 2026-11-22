#include "Counter.hpp"
#include <QtGui/QGuiApplication>
#include <QtQml/QQmlApplicationEngine>

int main(int argc, char** argv)
{
    QGuiApplication app(argc, argv);
    QQmlApplicationEngine engine;
    engine.loadFromModule("Easypp.Counter", "Main");
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }
    if (app.arguments().contains("--check")) {
        return engine.rootObjects().constFirst()->property("count").toInt() == 1 ? 0 : 1;
    }
    return app.exec();
}
