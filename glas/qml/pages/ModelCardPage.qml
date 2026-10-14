import QtQuick 2.6
import Sailfish.Silica 1.0

// Detail card for one thinking (LLM) model: description, size, quantization,
// parameters, language/engine and the fit badge (green/yellow/red), plus the
// download/select/remove action. All values come from Rust (src/llm.rs); the
// QML only maps the fit string to text/color and lays the card out.
Page {
    id: card

    property string modelId: ""
    property var model: ({})
    property string selectedId: ""
    property string ramHuman: ""
    property string diskHuman: ""

    property string name: (model && model.name) ? model.name
                                               : (modelId !== "" ? modelId : qsTr("Model"))
    property string fit: (model && model.fit !== undefined) ? model.fit : ""
    property bool downloading: (model && model.state === "downloading")
    property string stateError: "error"

    function llmCmd(cmd, val) {
        if (typeof bridge !== "undefined")
            bridge.llm_command(cmd, val === undefined ? "" : val)
    }

    function refresh() {
        if (typeof bridge === "undefined")
            return
        if (typeof bridge.llm_model_json === "function") {
            try {
                card.model = JSON.parse(bridge.llm_model_json(modelId))
            } catch (e) {
                card.model = {}
            }
        }
        try {
            var sv = JSON.parse(bridge.llm_status_json())
            card.selectedId = sv.llm || ""
            var dev = sv.device || {}
            card.ramHuman = dev.ram_human || ""
            card.diskHuman = dev.disk_human || ""
        } catch (e2) {
        }
    }

    function fitText(f) {
        if (f === "green")
            return qsTr("Fits comfortably")
        if (f === "yellow")
            return qsTr("Tight — may be slow")
        if (f === "red")
            return qsTr("Won't fit")
        return qsTr("Unknown")
    }

    function fitColor(f) {
        if (f === "green")
            return "#4caf50"
        if (f === "yellow")
            return "#ffb300"
        if (f === "red")
            return "#e53935"
        return Theme.secondaryColor
    }

    function detailRows() {
        var m = card.model || {}
        var rows = []
        if (m.size_human)
            rows.push({ k: qsTr("Size"), v: m.size_human })
        if (m.quant)
            rows.push({ k: qsTr("Quantization"), v: m.quant })
        if (m.params)
            rows.push({ k: qsTr("Parameters"), v: m.params })
        if (m.lang_name)
            rows.push({ k: qsTr("Language"), v: m.lang_name })
        if (m.engine)
            rows.push({ k: qsTr("Engine"), v: m.engine })
        if (m.need_human)
            rows.push({ k: qsTr("Needs (est.)"), v: "~" + m.need_human })
        if (m.model_id)
            rows.push({ k: qsTr("ID"), v: m.model_id })
        return rows
    }

    function isSelected() {
        var m = card.model || {}
        return m.model_id !== undefined && m.model_id === card.selectedId
    }

    function primaryText() {
        var m = card.model || {}
        if (m.state === "downloading")
            return qsTr("Cancel")
        if (m.downloaded)
            return card.isSelected() ? qsTr("In use") : qsTr("Use this model")
        return qsTr("Download")
    }

    function primaryEnabled() {
        var m = card.model || {}
        return !(m.downloaded && card.isSelected())
    }

    function primaryAction() {
        var m = card.model || {}
        if (!m.model_id)
            return
        if (m.state === "downloading")
            card.llmCmd("llm_download_cancel", m.model_id)
        else if (m.downloaded)
            card.llmCmd("llm_select", m.model_id)
        else
            card.llmCmd("llm_download", m.model_id)
        card.refresh()
    }

    Timer {
        interval: 500
        running: card.status === PageStatus.Active
        repeat: true
        onTriggered: card.refresh()
    }

    PageHeader {
        id: pageHeader
        title: card.name
    }

    SilicaFlickable {
        anchors {
            top: pageHeader.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        contentHeight: column.height + Theme.paddingLarge
        flickableDirection: Flickable.VerticalFlick

        Column {
            id: column
            width: parent.width

            // ── fit badge ────────────────────────────────────────────────
            Item {
                width: parent.width
                height: badgeRow.height + Theme.paddingLarge

                Row {
                    id: badgeRow
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    spacing: Theme.paddingSmall
                    anchors.verticalCenter: parent.verticalCenter

                    Rectangle {
                        width: Theme.paddingLarge
                        height: Theme.paddingLarge
                        radius: width / 2
                        color: card.fitColor(card.fit)
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Label {
                        text: card.fitText(card.fit)
                        color: card.fitColor(card.fit)
                        font.pixelSize: Theme.fontSizeMedium
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }

            // ── download progress ────────────────────────────────────────
            Column {
                width: parent.width
                spacing: Theme.paddingSmall
                visible: card.downloading

                ProgressBar {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    value: (card.model && card.model.progress !== undefined)
                           ? card.model.progress : 0
                }
                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    text: qsTr("Downloading… %1 / %2")
                          .arg((card.model && card.model.done_human) || "")
                          .arg((card.model && card.model.size_human) || "")
                    color: Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                }
            }

            // ── details ──────────────────────────────────────────────────
            Repeater {
                model: card.detailRows()

                delegate: Item {
                    width: column.width
                    height: Math.max(keyLabel.height, valueLabel.height)
                            + Theme.paddingSmall

                    Label {
                        id: keyLabel
                        x: Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.k
                        color: Theme.secondaryColor
                        font.pixelSize: Theme.fontSizeExtraSmall
                    }
                    Label {
                        id: valueLabel
                        anchors {
                            left: keyLabel.right
                            leftMargin: Theme.paddingMedium
                            right: parent.right
                            rightMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        text: modelData.v
                        wrapMode: Text.Wrap
                        horizontalAlignment: Text.AlignRight
                        color: Theme.primaryColor
                        font.pixelSize: Theme.fontSizeSmall
                    }
                }
            }

            // ── description ──────────────────────────────────────────────
            Item { width: 1; height: Theme.paddingMedium }
            Label {
                visible: !!(card.model && card.model.description
                            && card.model.description.length > 0)
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                text: (card.model && card.model.description) || ""
                color: Theme.primaryColor
                font.pixelSize: Theme.fontSizeSmall
            }

            // ── device ───────────────────────────────────────────────────
            Item { width: 1; height: Theme.paddingMedium }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                text: qsTr("Free now: %1 RAM · %2 disk")
                      .arg(card.ramHuman).arg(card.diskHuman)
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }

            // ── actions ──────────────────────────────────────────────────
            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                text: card.primaryText()
                enabled: card.primaryEnabled()
                onClicked: card.primaryAction()
            }

            Button {
                visible: !!(card.model && card.model.downloaded) && !card.downloading
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Remove")
                onClicked: {
                    card.llmCmd("llm_delete", card.model.model_id)
                    card.refresh()
                }
            }

            Item { width: 1; height: Theme.paddingLarge }
        }

        VerticalScrollDecorator {}
    }

    Component.onCompleted: card.refresh()
}
