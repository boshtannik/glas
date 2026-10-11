import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"

Page {
    id: page

    PullDownMenu {
        MenuItem {
            text: qsTr("Настройки")
            onClicked: pageStack.push(Qt.resolvedUrl("SettingsPage.qml"))
        }
        MenuItem {
            text: qsTr("Голосовые модели")
            onClicked: pageStack.push(Qt.resolvedUrl("VoiceModelsPage.qml"))
        }
        MenuItem {
            text: qsTr("Навыки")
            onClicked: pageStack.push(Qt.resolvedUrl("SkillsPage.qml"))
        }
    }

    Column {
        anchors.centerIn: parent
        spacing: Theme.paddingLarge
        width: parent.width

        MicButton {
            id: mic
            anchors.horizontalCenter: parent.horizontalCenter
            onClicked: {
                if (bridge !== undefined) {
                    bridge.request("(каркас: распознавание ещё не подключено)")
                }
            }
        }

        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width - 2 * Theme.horizontalPageMargin
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: app.answerText
            color: Theme.secondaryColor
        }
    }
}
