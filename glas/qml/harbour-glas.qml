import QtQuick 2.6
import Sailfish.Silica 1.0
import "pages"

ApplicationWindow {
    id: app

    // Bridge status / last answer, shown on the dialog page and the cover.
    property string statusText: (typeof bridge !== "undefined") ? bridge.status : "idle"
    property string answerText: (typeof bridge !== "undefined") ? bridge.answer : ""

    initialPage: Component { DialogPage { } }

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
