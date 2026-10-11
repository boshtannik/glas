import QtQuick 2.6
import Sailfish.Silica 1.0
import "../components"

Item {
    id: dialogPage

    property var appWindow

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PageHeader {
            id: pageHeader
            title: qsTr("Glas")
        }

        Column {
            id: column
            anchors {
                top: pageHeader.bottom
                left: parent.left
                right: parent.right
            }
            spacing: Theme.paddingLarge

            MicButton {
                id: mic
                anchors.horizontalCenter: parent.horizontalCenter
                onClicked: {
                    if (bridge !== undefined)
                        bridge.request("(skeleton: recognition not wired yet)")
                }
            }

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
}
