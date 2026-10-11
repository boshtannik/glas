import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"

Item {
    id: dialogPage

    property var appWindow

    PageHeader {
        id: pageHeader
        title: qsTr("Glas")
    }

    // Transcript / status area above the push-to-talk button.
    SilicaFlickable {
        id: transcript
        anchors {
            top: pageHeader.bottom
            left: parent.left
            right: parent.right
            bottom: inputPanel.top
        }
        contentHeight: column.height + Theme.paddingLarge
        clip: true

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingLarge
            topPadding: Theme.paddingLarge

            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: dialogPage.appWindow.statusText
                color: Theme.secondaryColor
            }

            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: dialogPage.appWindow.answerText
                color: Theme.highlightColor
            }
        }
    }

    // Bottom push-to-talk panel (mic anchored to the bottom, as in the reference).
    Item {
        id: inputPanel
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        height: mic.height + Theme.paddingLarge

        MicButton {
            id: mic
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: parent.bottom
                bottomMargin: Theme.paddingLarge
            }
            onPressed: {
                if (bridge !== undefined)
                    bridge.request("(skeleton: push-to-talk start)")
            }
            onReleased: {
                if (bridge !== undefined)
                    bridge.cancel()
            }
        }
    }
}
