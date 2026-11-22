import QtQuick
import "labels.js" as Labels

Text {
    required property int count
    text: Labels.counterText(count)
    font.pixelSize: 30
}
