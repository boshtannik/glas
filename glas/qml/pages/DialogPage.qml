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

    // Dialogue history, mirrored from the bridge's JSON property.
    ListModel { id: historyModel }

    function rebuildHistory() {
        if (typeof bridge === "undefined")
            return
        var items = []
        try {
            items = JSON.parse(bridge.history)
        } catch (e) {
            items = []
        }
        historyModel.clear()
        for (var i = 0; i < items.length; i++)
            historyModel.append(items[i])
        historyView.positionViewAtEnd()
    }

    Connections {
        target: (typeof bridge !== "undefined") ? bridge : null
        onHistory_changed: dialogPage.rebuildHistory()
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

    // Transcript (history bubbles) + the live status bubble, above push-to-talk.
    Item {
        id: transcript
        anchors {
            top: pageHeader.bottom
            left: parent.left
            right: parent.right
            bottom: inputPanel.top
        }

        // One bubble per dialogue line; the user's on the right, Glas's on the left.
        ListView {
            id: historyView
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
                bottom: parent.bottom
                bottomMargin: liveBubble.visible
                    ? liveBubble.height + 2 * Theme.paddingMedium
                    : 0
            }
            clip: true
            spacing: Theme.paddingMedium
            topMargin: Theme.paddingLarge
            bottomMargin: Theme.paddingMedium
            model: historyModel

            delegate: Item {
                width: historyView.width
                height: bubble.height

                Rectangle {
                    id: bubble
                    property bool isUser: model.role === "user"
                    anchors.top: parent.top
                    anchors.left: isUser ? undefined : parent.left
                    anchors.right: isUser ? parent.right : undefined
                    anchors.leftMargin: Theme.horizontalPageMargin
                    anchors.rightMargin: Theme.horizontalPageMargin
                    width: Math.min(bubbleLabel.implicitWidth, historyView.width * 0.78)
                        + 2 * Theme.paddingMedium
                    height: bubbleLabel.height + 2 * Theme.paddingMedium
                    radius: Theme.paddingMedium
                    color: isUser
                        ? Theme.rgba(Theme.highlightColor, 0.22)
                        : Theme.rgba(Theme.primaryColor, 0.10)

                    Label {
                        id: bubbleLabel
                        x: Theme.paddingMedium
                        y: Theme.paddingMedium
                        width: Math.min(implicitWidth, historyView.width * 0.78)
                        wrapMode: Text.Wrap
                        text: model.text
                        font.pixelSize: Theme.fontSizeSmall
                        color: Theme.primaryColor
                    }
                }
            }
        }

        // Live status: the current stage of the turn, updated in real time
        // ("Слушаю…" → "Перевожу в текст…" → "Обдумываю…" → "Озвучиваю ответ…").
        Rectangle {
            id: liveBubble
            visible: liveText() !== ""
            anchors {
                left: parent.left
                bottom: parent.bottom
                leftMargin: Theme.horizontalPageMargin
                bottomMargin: Theme.paddingMedium
            }
            width: Math.min(liveLabel.implicitWidth, historyView.width * 0.78)
                + 2 * Theme.paddingMedium
            height: liveLabel.height + 2 * Theme.paddingMedium
            radius: Theme.paddingMedium
            color: Theme.rgba(Theme.highlightColor, 0.18)

            function liveText() {
                if (typeof bridge === "undefined")
                    return ""
                if (bridge.stage !== "")
                    return bridge.stage
                if (dialogPage.speaking)
                    return "Озвучиваю ответ…"
                return ""
            }

            Label {
                id: liveLabel
                x: Theme.paddingMedium
                y: Theme.paddingMedium
                width: Math.min(implicitWidth, historyView.width * 0.78)
                wrapMode: Text.Wrap
                text: liveBubble.liveText()
                font.pixelSize: Theme.fontSizeSmall
                font.italic: true
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
