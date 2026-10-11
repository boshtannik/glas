import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"

Page {
    id: dialogPage

    PageHeader {
        id: pageHeader
        title: qsTr("Glas")
    }

    // Always-visible action in the header: no need to scroll the transcript
    // to reach the settings. `extraContent` is a read-only slot, so the button
    // is reparented into it (as in system apps).
    IconButton {
        parent: pageHeader.extraContent
        anchors {
            left: parent.left
            verticalCenter: parent.verticalCenter
        }
        icon.source: "image://theme/icon-m-menu"
        onClicked: pageStack.push(Qt.resolvedUrl("SettingsPage.qml"))
    }

    // Copy the transcription result from the background worker into the UI.
    Timer {
        interval: 250
        running: true
        repeat: true
        onTriggered: {
            if (typeof bridge !== "undefined")
                bridge.poll()
        }
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
                text: (typeof bridge !== "undefined") ? bridge.status : "idle"
                color: Theme.secondaryColor
            }

            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: (typeof bridge !== "undefined") ? bridge.answer : ""
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
                if (typeof bridge !== "undefined")
                    bridge.ptt_start()
            }
            onReleased: {
                if (typeof bridge !== "undefined")
                    bridge.ptt_stop()
            }
        }
    }
}
