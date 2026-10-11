import QtQuick 2.6
import Sailfish.Silica 1.0
import "pages"

ApplicationWindow {
    id: app

    // Bridge status / last answer, shown in the dialog and on the cover.
    property string statusText: (bridge !== undefined) ? bridge.status : "idle"
    property string answerText: (bridge !== undefined) ? bridge.answer : ""

    // Carousel: [0] Dialog | [1] Settings | [2] Voice models | [3] Skills.
    // PagedView gives horizontal swipe + snap (as in the reference).
    initialPage: Component {
        Page {
            PagedView {
                id: pager
                anchors.fill: parent
                currentIndex: 0
                wrapMode: PagedView.NoWrap
                model: [dialogPage, settingsPage, voiceModelsPage, skillsPage]

                delegate: Item {
                    width: PagedView.contentWidth
                    height: PagedView.contentHeight

                    Loader {
                        anchors.fill: parent
                        sourceComponent: modelData
                    }
                }

                Component { id: dialogPage; DialogPage { appWindow: app } }
                Component { id: settingsPage; SettingsPage { appWindow: app } }
                Component { id: voiceModelsPage; VoiceModelsPage { appWindow: app } }
                Component { id: skillsPage; SkillsPage { appWindow: app } }
            }

            // Dot indicator (SFOS 5.1 has no ready-made PageIndicator).
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: parent.top
                anchors.topMargin: Theme.paddingMedium
                spacing: Theme.paddingSmall
                z: 10

                Repeater {
                    model: pager.count
                    Rectangle {
                        width: Theme.paddingSmall
                        height: width
                        radius: width / 2
                        color: index === pager.currentIndex
                               ? Theme.primaryColor : Theme.secondaryColor
                    }
                }
            }
        }
    }

    cover: Component {
        CoverBackground {
            Column {
                anchors.centerIn: parent
                spacing: Theme.paddingMedium
                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    source: "image://theme/icon-m-mic"
                }
                Label {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: app.statusText
                }
            }
        }
    }
}
