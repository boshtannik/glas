import QtQuick 2.6
import Sailfish.Silica 1.0
import "pages"

ApplicationWindow {
    id: app

    initialPage: DialogPage {}

    // The bridge status is shown both on the cover and in the dialog.
    property string statusText: (bridge !== undefined) ? bridge.status : "idle"
    // Last assistant answer.
    property string answerText: (bridge !== undefined) ? bridge.answer : ""

    cover: Component {
        CoverBackground {
            Column {
                anchors.centerIn: parent
                spacing: Theme.paddingMedium
                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    source: "image://theme/icon-l-mic"
                }
                Label {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: app.statusText
                }
            }
        }
    }
}
