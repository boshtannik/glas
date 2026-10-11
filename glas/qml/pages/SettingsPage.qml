import QtQuick 2.6
import Sailfish.Silica 1.0

Page {
    id: page

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader { title: qsTr("Настройки") }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                text: qsTr("Каркас: переключатели языка, TTS, окна контекста и выгрузки моделей появятся позже.")
                color: Theme.secondaryColor
            }
        }

        VerticalScrollDecorator {}
    }
}
