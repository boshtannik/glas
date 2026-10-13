import QtQuick 2.6
import Sailfish.Silica 1.0

Page {
    id: settingsPage

    PageHeader {
        id: pageHeader
        title: qsTr("Settings")
    }

    // One entry point for every category; picking one opens its editor only.
    SilicaListView {
        anchors {
            top: pageHeader.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        model: [
            { title: qsTr("General"), page: "GeneralSettingsPage.qml" },
            { title: qsTr("Voice models"), page: "VoiceModelsPage.qml" },
            { title: qsTr("Thinking models"), page: "ThinkingModelsPage.qml" },
            { title: qsTr("Skills"), page: "SkillsPage.qml" },
            { title: qsTr("Memory"), page: "MemoryPage.qml" }
        ]
        delegate: BackgroundItem {
            width: ListView.view.width
            height: Theme.itemSizeMedium

            Label {
                anchors.verticalCenter: parent.verticalCenter
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: modelData.title
                color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
            }

            onClicked: pageStack.push(Qt.resolvedUrl(modelData.page))
        }

        VerticalScrollDecorator {}
    }
}
