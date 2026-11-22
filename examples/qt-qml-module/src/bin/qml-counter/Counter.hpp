#pragma once

#include <QtCore/QObject>
#include <QtQml/qqmlregistration.h>

class Counter : public QObject {
    Q_OBJECT
    QML_ELEMENT
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
