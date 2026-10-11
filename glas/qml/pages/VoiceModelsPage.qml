import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: voiceModelsPage

    property var appWindow

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PageHeader {
            id: pageHeader
            title: qsTr("Voice models")
        }

        Column {
            id: column
            anchors {
                top: pageHeader.bottom
                left: parent.left
                right: parent.right
            }
            spacing: Theme.paddingMedium

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                text: qsTr("Skeleton: the GGUF model catalog and memory-fit checks are coming later.")
                color: Theme.secondaryColor
            }
        }
    }
}
