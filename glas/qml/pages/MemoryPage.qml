import QtQuick 2.6
import Sailfish.Silica 1.0

Page {
    id: memoryPage

    // Facts parsed from the Rust bridge (`{key, value}`), sorted by key.
    property var facts: []

    // The memory bridge is only present in builds that include glas-memory.
    readonly property bool memReady: (typeof bridge !== "undefined")
                                     && (typeof bridge.memory_facts === "function")

    function reload() {
        var raw = memReady ? bridge.memory_facts() : "{}"
        var obj = {}
        try {
            obj = JSON.parse(raw)
        } catch (e) {
            obj = {}
        }
        var list = []
        if (obj && obj.facts) {
            for (var key in obj.facts)
                list.push({ key: key, value: obj.facts[key].value })
        }
        list.sort(function (a, b) {
            return a.key < b.key ? -1 : (a.key > b.key ? 1 : 0)
        })
        facts = list
    }

    function add() {
        var key = keyField.text.trim()
        var value = valueField.text.trim()
        if (key.length === 0 || value.length === 0) {
            note.text = qsTr("Both key and value are required.")
            return
        }
        if (!memReady) {
            note.text = qsTr("Memory is unavailable in this build.")
            return
        }
        var err = bridge.memory_set(key, value)
        if (err && err.length > 0) {
            note.text = err
        } else {
            keyField.text = ""
            valueField.text = ""
            note.text = ""
            reload()
        }
    }

    Component.onCompleted: reload()

    PageHeader {
        id: pageHeader
        title: qsTr("Memory")
    }

    SilicaFlickable {
        anchors {
            top: pageHeader.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            SectionHeader { text: qsTr("Add a preference") }

            TextField {
                id: keyField
                width: parent.width
                placeholderText: qsTr("key (a-z, 0-9, '_', '.')")
                EnterKey.enabled: text.length > 0
            }

            TextField {
                id: valueField
                width: parent.width
                placeholderText: qsTr("value")
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: memoryPage.add()
            }

            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Remember")
                enabled: keyField.text.trim().length > 0
                         && valueField.text.trim().length > 0
                onClicked: memoryPage.add()
            }

            Label {
                id: note
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.secondaryHighlightColor
            }

            SectionHeader { text: qsTr("Known facts") }

            Label {
                visible: memoryPage.facts.length === 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                text: qsTr("Nothing remembered yet.")
                color: Theme.secondaryColor
            }

            Repeater {
                model: memoryPage.facts

                delegate: ListItem {
                    width: column.width
                    contentHeight: Theme.itemSizeMedium

                    Label {
                        x: Theme.horizontalPageMargin
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        wrapMode: Text.Wrap
                        color: Theme.primaryColor
                        text: modelData.key + " = " + modelData.value
                    }

                    menu: ContextMenu {
                        MenuItem {
                            text: qsTr("Forget")
                            onClicked: {
                                if (memoryPage.memReady)
                                    bridge.memory_forget(modelData.key)
                                memoryPage.reload()
                            }
                        }
                    }
                }
            }
        }

        VerticalScrollDecorator {}
    }
}
