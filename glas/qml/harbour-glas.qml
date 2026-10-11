import QtQuick 2.6
import Sailfish.Silica 1.0
import "pages"

ApplicationWindow {
    id: app

    initialPage: DialogPage {}

    // Статус моста виден и на обложке, и в диалоге.
    property string statusText: (bridge !== undefined) ? bridge.status : "idle"
    // Последний ответ ассистента.
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
