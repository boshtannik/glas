import QtQuick 2.6
import Sailfish.Silica 1.0

Page {
    id: page

    PageHeader {
        id: pageHeader
        title: qsTr("Voice models")
    }

    SilicaFlickable {
        anchors {
            top: pageHeader.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        contentHeight: label.height + Theme.paddingLarge

        Label {
            id: label
            x: Theme.horizontalPageMargin
            y: Theme.paddingLarge
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            color: Theme.secondaryColor
            text: qsTr("Skeleton: the GGUF model catalog and memory-fit checks are coming later.")
        }
    }
}
