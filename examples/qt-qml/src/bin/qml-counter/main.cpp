#include <QtGui/QGuiApplication>
#include <QtQml/QQmlApplicationEngine>
#include <QtQml/qqml.h>

class Counter : public QObject {
    Q_OBJECT
    Q_PROPERTY(int value READ value NOTIFY valueChanged)

public:
    [[nodiscard]] int value() const { return value_; }

    Q_INVOKABLE void increment()
    {
        ++value_;
        emit valueChanged();
    }

signals:
    void valueChanged();

private:
    int value_ = 0;
};

int main(int argc, char** argv)
{
    QGuiApplication app(argc, argv);
    qmlRegisterType<Counter>("Easypp.Counter", 1, 0, "Counter");

    QQmlApplicationEngine engine;
    engine.load(QUrl("qrc:/qml/Main.qml"));
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }
    if (app.arguments().contains("--check")) {
        const auto* window = engine.rootObjects().constFirst();
        return window->property("count").toInt() == 1 && window->property("labelText").toString() == "Count: 1" ? 0 : 1;
    }
    return app.exec();
}

#include "main.moc"
