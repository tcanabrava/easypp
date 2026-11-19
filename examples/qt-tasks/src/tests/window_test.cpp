// Builds the real window offscreen: checks the widgets uic created, the
// resources rcc compiled into the window library, and the signal wiring.

#include "window/mainwindow.hpp"

#include <QApplication>
#include <QFile>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QPushButton>
#include <QTest>

class WindowTest : public QObject {
    Q_OBJECT

private slots:
    void resourcesAreLinkedIn()
    {
        QVERIFY(MainWindow::aboutText().contains("easypp"));
        QVERIFY(QFile::exists(":/icons/app.svg"));
    }

    void addingThroughTheUi()
    {
        TaskList tasks;
        MainWindow window(tasks);
        auto* input = window.findChild<QLineEdit*>("newTask");
        auto* add = window.findChild<QPushButton*>("addButton");
        auto* list = window.findChild<QListWidget*>("list");
        auto* count = window.findChild<QLabel*>("countLabel");
        QVERIFY(input && add && list && count);

        QTest::keyClicks(input, "water the plants");
        QTest::mouseClick(add, Qt::LeftButton);
        QCOMPARE(tasks.count(), 1);
        QCOMPARE(list->count(), 1);
        QCOMPARE(list->item(0)->text(), QString("water the plants"));
        QVERIFY(input->text().isEmpty());

        list->item(0)->setCheckState(Qt::Checked);
        QVERIFY(tasks.at(0).done);
        QCOMPARE(count->text(), QString("0 of 1 left"));
    }
};

int main(int argc, char* argv[])
{
    qputenv("QT_QPA_PLATFORM", "offscreen"); // no display needed
    QApplication app(argc, argv);
    WindowTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "window_test.moc"
