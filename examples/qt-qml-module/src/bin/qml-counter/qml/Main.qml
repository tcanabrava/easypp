import QtQuick
import Easypp.Counter

Window {
    id: window
    width: 360
    height: 220
    visible: true
    title: "QML Counter"

    readonly property int count: counter.value
    readonly property alias labelText: label.text

    Counter {
        id: counter
    }

    Component.onCompleted: counter.increment()

    CounterLabel {
        id: label
        anchors.centerIn: parent
        count: window.count
    }

    TapHandler {
        onTapped: counter.increment()
    }
}
