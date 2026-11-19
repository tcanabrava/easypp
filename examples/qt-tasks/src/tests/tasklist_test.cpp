// A Qt Test case whose Q_OBJECT class lives in this .cpp file: easypp runs
// moc on it and generates tasklist_test.moc, included at the end.

#include "tasks/tasklist.hpp"

#include <QSignalSpy>
#include <QTest>

class TaskListTest : public QObject {
    Q_OBJECT

private slots:
    void addsAndIgnoresBlankTitles()
    {
        TaskList tasks;
        QSignalSpy changed(&tasks, &TaskList::changed);
        tasks.add("  buy milk  ");
        tasks.add("   ");
        QCOMPARE(tasks.count(), 1);
        QCOMPARE(tasks.at(0).title, QString("buy milk"));
        QCOMPARE(changed.count(), 1);
    }

    void tracksDoneTasks()
    {
        TaskList tasks;
        tasks.add("a");
        tasks.add("b");
        tasks.add("c");
        tasks.setDone(1, true);
        QCOMPARE(tasks.remaining(), 2);
        tasks.clearDone();
        QCOMPARE(tasks.count(), 2);
        QCOMPARE(tasks.at(1).title, QString("c"));
    }

    void propertiesWorkThroughTheMetaObject()
    {
        TaskList tasks;
        tasks.add("x");
        tasks.add("y");
        tasks.setDone(0, true);
        // Only works if moc ran on tasklist.hpp.
        QCOMPARE(tasks.property("count").toInt(), 2);
        QCOMPARE(tasks.property("remaining").toInt(), 1);
        QVERIFY(QMetaObject::invokeMethod(&tasks, "clearDone"));
        QCOMPARE(tasks.count(), 1);
    }

    void roundTripsJson()
    {
        TaskList tasks;
        tasks.add("one");
        tasks.add("two");
        tasks.setDone(1, true);
        TaskList copy;
        QVERIFY(copy.fromJson(tasks.toJson()));
        QCOMPARE(copy.count(), 2);
        QVERIFY(copy.at(1).done);
        QVERIFY(!copy.fromJson("{\"not\": \"a list\"}"));
        QCOMPARE(copy.count(), 2);
    }
};

QTEST_GUILESS_MAIN(TaskListTest)
#include "tasklist_test.moc"
