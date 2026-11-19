#pragma once

// The window: Qt Widgets. Its layout comes from mainwindow.ui (uic) and its
// icon and About text from resources.qrc (rcc); easypp runs both, plus moc
// for the Q_OBJECT below.

#include "tasks/tasklist.hpp"

#include <QMainWindow>

#include <memory>

namespace Ui {
class MainWindow;
}

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(TaskList& tasks, QWidget* parent = nullptr);
    ~MainWindow() override;

    // Text of the bundled About file (from the Qt resource system).
    static QString aboutText();

private slots:
    void addFromInput();
    void removeSelected();
    void refresh();
    void showAbout();

private:
    std::unique_ptr<Ui::MainWindow> ui_;
    TaskList& tasks_;
    bool refreshing_ = false;
};
