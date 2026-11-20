#include "tasklist.hpp"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

#include <algorithm>

TaskList::TaskList(QObject* parent)
    : QObject(parent)
{
}

int TaskList::count() const
{
    return static_cast<int>(tasks_.size());
}

int TaskList::remaining() const
{
    return static_cast<int>(std::count_if(tasks_.begin(), tasks_.end(), [](const Task& t) { return !t.done; }));
}

const Task& TaskList::at(int index) const
{
    return tasks_.at(index);
}

void TaskList::add(const QString& title)
{
    const QString trimmed = title.trimmed();
    if (trimmed.isEmpty()) {
        return;
    }
    tasks_.append(Task {trimmed, false});
    emit changed();
}

void TaskList::remove(int index)
{
    if (index < 0 || index >= count()) {
        return;
    }
    tasks_.removeAt(index);
    emit changed();
}

void TaskList::setDone(int index, bool done)
{
    if (index < 0 || index >= count() || tasks_[index].done == done) {
        return;
    }
    tasks_[index].done = done;
    emit changed();
}

void TaskList::clearDone()
{
    const auto removed = tasks_.removeIf([](const Task& t) { return t.done; });
    if (removed > 0) {
        emit changed();
    }
}

QByteArray TaskList::toJson() const
{
    QJsonArray array;
    for (const Task& task : tasks_) {
        array.append(QJsonObject {{"title", task.title}, {"done", task.done}});
    }
    return QJsonDocument(array).toJson(QJsonDocument::Compact);
}

bool TaskList::fromJson(const QByteArray& json)
{
    const QJsonDocument document = QJsonDocument::fromJson(json);
    if (!document.isArray()) {
        return false;
    }
    QList<Task> loaded;
    for (const QJsonValue& value : document.array()) {
        const QJsonObject object = value.toObject();
        if (!object.value("title").isString()) {
            return false;
        }
        loaded.append(Task {object.value("title").toString(), object.value("done").toBool()});
    }
    tasks_ = loaded;
    emit changed();
    return true;
}
