#pragma once

// The model: Qt Core only (no widgets), with signals, slots and a property.
// Q_OBJECT makes easypp run moc on this header; the generated
// moc_tasklist.cpp is compiled into this library.

#include <QByteArray>
#include <QList>
#include <QObject>
#include <QString>

struct Task {
    QString title;
    bool done = false;
};

class TaskList : public QObject {
    Q_OBJECT
    Q_PROPERTY(int count READ count NOTIFY changed)
    Q_PROPERTY(int remaining READ remaining NOTIFY changed)

public:
    explicit TaskList(QObject* parent = nullptr);

    int count() const;
    int remaining() const;
    const Task& at(int index) const;

    QByteArray toJson() const;
    // Replaces the list; returns false (and changes nothing) on bad input.
    bool fromJson(const QByteArray& json);

public slots:
    // Blank titles are ignored.
    void add(const QString& title);
    void remove(int index);
    void setDone(int index, bool done);
    void clearDone();

signals:
    void changed();

private:
    QList<Task> tasks_;
};
