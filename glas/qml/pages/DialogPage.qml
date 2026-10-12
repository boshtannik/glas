import QtQuick 2.6
import Sailfish.Silica 1.0
import QtMultimedia 5.4
import "../components"

Page {
    id: dialogPage

    // True while the synthesized answer is being played back.
    property bool speaking: false

    // Playback is done here in QML (not in Rust): the worker only synthesizes
    // the WAV and hands over its path (bridge.tts_file), exactly as the
    // reference app does — so the audio thread never blocks the worker.
    //
    // NOTE: qmetaobject exposes names verbatim (snake_case), so the handler is
    // `onTts_file_changed` and the property is `bridge.tts_file` (Qt only
    // capitalises the first letter of the signal name).
    MediaPlayer {
        id: ttsPlayer
        onPlaybackStateChanged: {
            if (playbackState === MediaPlayer.StoppedState)
                dialogPage.speaking = false
        }
    }

    Connections {
        target: (typeof bridge !== "undefined") ? bridge : null
        onTts_file_changed: {
            if (bridge.tts_file === "")
                return
            ttsPlayer.source = "file://" + bridge.tts_file
            dialogPage.speaking = true
            ttsPlayer.play()
        }
    }

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

        // Speak / stop the last answer (the worker synthesizes, we play).
        IconButton {
            id: speakButton
            anchors {
                right: mic.left
                rightMargin: Theme.paddingLarge
                verticalCenter: mic.verticalCenter
            }
            visible: (typeof bridge !== "undefined")
                && bridge.answer !== ""
                && bridge.status !== "recording"
                && bridge.status !== "processing"
            icon.source: dialogPage.speaking
                ? "image://theme/icon-m-stop"
                : "image://theme/icon-m-speaker"
            onClicked: {
                if (typeof bridge === "undefined")
                    return
                if (dialogPage.speaking) {
                    dialogPage.speaking = false
                    ttsPlayer.stop()
                    bridge.tts_stop()
                } else {
                    bridge.tts_speak(bridge.answer)
                }
            }
        }

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
