import QtQuick 2.6
import Sailfish.Silica 1.0

Page {
    id: page

    PageHeader {
        id: pageHeader
        title: qsTr("General")
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
            text: qsTr("Skeleton: language, TTS, context-window and model-unload switches are coming later.")
        }
    }
}
