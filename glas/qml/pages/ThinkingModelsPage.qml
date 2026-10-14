import QtQuick 2.6
import Sailfish.Silica 1.0

// Thinking (LLM) models: a search field, a language filter, a downloaded
// section and the catalog. All selection/filtering/fit logic lives in Rust
// (src/llm.rs); the QML only renders the snapshot and forwards commands.
// Tapping a row opens the detail card (ModelCardPage.qml).
Page {
    id: llmPage

    // Snapshot of the bridge LLM store.
    property var dlModels: []       // downloaded (already filtered by Rust)
    property var catModels: []      // available catalog (already filtered)
    property var langOptions: [ { id: "auto", name: qsTr("Auto") } ]
    property string chosenLlm: ""
    property string version: ""
    property string query: ""
    property string ramHuman: ""
    property string diskHuman: ""
    property int onDeviceCount: 0
    property string onDeviceHuman: ""
    property bool updating: false
    property string updateError: ""

    property string filterLang: "auto"
    property bool dlExpanded: true
    property bool llmExpanded: true

    // Protocol constants — must match src/llm.rs.
    readonly property string cmdSelect: "llm_select"
    readonly property string cmdDownload: "llm_download"
    readonly property string cmdDownloadCancel: "llm_download_cancel"
    readonly property string cmdDelete: "llm_delete"
    readonly property string cmdSearch: "llm_search"
    readonly property string cmdFilterLang: "llm_filter_lang"
    readonly property string cmdCatalogUpdate: "voice_catalog_update"
    readonly property string stateDownloading: "downloading"
    readonly property string stateError: "error"

    readonly property string secOndevice: "ondevice"
    readonly property string secLlm: "llm"

    // Short labels for the fit badge (display-only; the value comes from Rust).
    readonly property string fitGreen: "green"
    readonly property string fitYellow: "yellow"
    readonly property string fitRed: "red"

    // Structural fingerprint: the list is rebuilt only when membership/state/
    // selection/sections change, not on every 400 ms poll.
    property string llmFp: fingerprint()
    onLlmFpChanged: fillModel()

    ListModel { id: llmModel }

    Timer {
        interval: 400
        running: llmPage.status === PageStatus.Active
        repeat: true
        onTriggered: llmPage.refresh()
    }

    function refresh() {
        if (typeof bridge === "undefined")
            return
        var s = bridge.llm_status_json()
        if (!s || s === "")
            return
        try {
            var vo = JSON.parse(s)
            llmPage.dlModels = vo.downloaded || []
            llmPage.catModels = vo.catalog || []
            llmPage.chosenLlm = vo.llm || ""
            llmPage.version = vo.version || ""
            llmPage.query = vo.query || ""
            var dev = vo.device || {}
            llmPage.ramHuman = dev.ram_human || ""
            llmPage.diskHuman = dev.disk_human || ""
            var od = vo.on_device || {}
            llmPage.onDeviceCount = od.count ? od.count : 0
            llmPage.onDeviceHuman = od.human || ""
            llmPage.updating = !!vo.updating
            llmPage.updateError = vo.update_error || ""
            if (vo.langs && vo.langs.length) {
                var opts = [{ id: "auto", name: qsTr("Auto") }]
                for (var i = 0; i < vo.langs.length; i++) {
                    var L = vo.langs[i]
                    opts.push({ id: L.id, name: L.name || L.id })
                }
                llmPage.langOptions = opts
            }
        } catch (e) {
            console.log("llm parse error: " + e)
        }
    }

    function llmCmd(cmd, val) {
        if (typeof bridge !== "undefined")
            bridge.llm_command(cmd, val === undefined ? "" : val)
    }

    function fingerprint() {
        var out = ""
        var a = llmPage.dlModels || []
        for (var i = 0; i < a.length; i++)
            out += a[i].model_id + "|" + a[i].state + ";"
        var b = llmPage.catModels || []
        for (var j = 0; j < b.length; j++)
            out += b[j].model_id + "|" + b[j].state + ";"
        return out + "@" + llmPage.chosenLlm + "%" + llmPage.llmExpanded
               + "^" + llmPage.dlExpanded
    }

    function filterIndex() {
        var o = llmPage.langOptions || []
        for (var i = 0; i < o.length; i++)
            if (o[i].id === llmPage.filterLang)
                return i
        return 0
    }

    function isSelected(m) {
        return m.model_id === llmPage.chosenLlm
    }

    function rowSub(m) {
        var s = (m.lang_name || "") + " · " + (m.size_human || "")
        if (m.quant)
            s += " · " + m.quant
        if (m.state === llmPage.stateError)
            s += " · " + qsTr("download failed")
        else if (m.downloaded)
            s += " · " + qsTr("downloaded")
        return s
    }

    function glyph(m) {
        if (m.state === llmPage.stateDownloading)
            return "✕"
        if (m.state === llmPage.stateError)
            return "!"
        if (llmPage.isSelected(m))
            return "✓"
        if (m.downloaded)
            return "→"
        return "↓"
    }

    function glyphColor(m) {
        if (m.state === llmPage.stateDownloading || m.state === llmPage.stateError)
            return Theme.secondaryColor
        if (llmPage.isSelected(m))
            return Theme.highlightColor
        return Theme.primaryColor
    }

    function fitColor(f) {
        if (f === llmPage.fitGreen)
            return "#4caf50"
        if (f === llmPage.fitYellow)
            return "#ffb300"
        if (f === llmPage.fitRed)
            return "#e53935"
        return "transparent"
    }

    function findModel(id) {
        var a = llmPage.dlModels || []
        for (var i = 0; i < a.length; i++)
            if (a[i].model_id === id)
                return a[i]
        var b = llmPage.catModels || []
        for (var j = 0; j < b.length; j++)
            if (b[j].model_id === id)
                return b[j]
        return null
    }

    function liveInfo(id) {
        var m = llmPage.findModel(id)
        if (!m || m.state !== llmPage.stateDownloading)
            return ""
        var pct = Math.min(99, Math.round((m.progress || 0) * 100))
        return qsTr("Downloading… %1% (%2 / %3)")
               .arg(pct).arg(m.done_human || "").arg(m.size_human || "")
    }

    function liveRatio(id) {
        var m = llmPage.findModel(id)
        if (!m || m.state !== llmPage.stateDownloading)
            return 0
        return m.progress || 0
    }

    function fillModel() {
        llmModel.clear()

        var dl = llmPage.dlModels || []
        llmModel.append({ isHeader: true, kind: llmPage.secOndevice,
                          title: qsTr("Downloaded models"), count: dl.length,
                          name: "", sub: "", dlstate: "", modelId: "",
                          fit: "", downloaded: false })
        if (llmPage.dlExpanded)
            for (var i = 0; i < dl.length; i++)
                llmPage.appendRow(dl[i])

        var ct = llmPage.catModels || []
        llmModel.append({ isHeader: true, kind: llmPage.secLlm,
                          title: qsTr("Thinking models"), count: ct.length,
                          name: "", sub: "", dlstate: "", modelId: "",
                          fit: "", downloaded: false })
        if (llmPage.llmExpanded)
            for (var j = 0; j < ct.length; j++)
                llmPage.appendRow(ct[j])
    }

    function appendRow(m) {
        llmModel.append({ isHeader: false, kind: "",
                          title: "", count: 0,
                          name: m.name, sub: llmPage.rowSub(m),
                          dlstate: m.state, modelId: m.model_id,
                          fit: m.fit, downloaded: !!m.downloaded })
    }

    function toggleSection(kind) {
        if (kind === llmPage.secLlm)
            llmPage.llmExpanded = !llmPage.llmExpanded
        else
            llmPage.dlExpanded = !llmPage.dlExpanded
    }

    function openCard(modelId) {
        pageStack.push(Qt.resolvedUrl("ModelCardPage.qml"), { modelId: modelId })
    }

    function quickAction(modelId, dlstate, downloaded) {
        if (dlstate === llmPage.stateDownloading)
            llmPage.llmCmd(llmPage.cmdDownloadCancel, modelId)
        else if (downloaded)
            llmPage.llmCmd(llmPage.cmdSelect, modelId)
        else
            llmPage.llmCmd(llmPage.cmdDownload, modelId)
    }

    function onRowTap(isHeader, kind, modelId) {
        if (isHeader) {
            llmPage.toggleSection(kind)
            return
        }
        llmPage.openCard(modelId)
    }

    function onRowMenu(isHeader, kind, modelId, dlstate, downloaded, name, wdg) {
        if (isHeader) {
            llmPage.toggleSection(kind)
            return
        }
        if (dlstate === llmPage.stateDownloading)
            llmPage.llmCmd(llmPage.cmdDownloadCancel, modelId)
        else if (downloaded)
            remorse.execute(wdg, qsTr("Removing «%1»…").arg(name), function() {
                llmPage.llmCmd(llmPage.cmdDelete, modelId)
            })
        else
            llmPage.llmCmd(llmPage.cmdDownload, modelId)
    }

    RemorseItem { id: remorse }

    SilicaListView {
        id: list
        anchors.fill: parent
        model: llmModel

        header: Item {
            width: parent.width
            height: searchField.height + filterCombo.height
                    + summaryLabel.height + legendLabel.height
                    + (updateLabel.visible ? updateLabel.height + Theme.paddingSmall : 0)
                    + Theme.paddingMedium

            SearchField {
                id: searchField
                width: parent.width
                placeholderText: qsTr("Search thinking models")
                onTextChanged: {
                    llmPage.llmCmd(llmPage.cmdSearch, text)
                    llmPage.refresh()
                }
            }

            ComboBox {
                id: filterCombo
                anchors.top: searchField.bottom
                width: parent.width
                label: qsTr("Language")
                currentIndex: llmPage.filterIndex()
                menu: ContextMenu {
                    Repeater {
                        model: llmPage.langOptions
                        MenuItem { text: modelData.name }
                    }
                }
                onCurrentIndexChanged: {
                    var o = llmPage.langOptions
                    if (currentIndex < 0 || currentIndex >= o.length)
                        return
                    llmPage.filterLang = o[currentIndex].id
                    llmPage.llmCmd(llmPage.cmdFilterLang, llmPage.filterLang)
                    llmPage.refresh()
                }
            }

            Label {
                id: summaryLabel
                anchors.top: filterCombo.bottom
                anchors.topMargin: Theme.paddingSmall
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("On device: %1, %2")
                      .arg(llmPage.onDeviceCount)
                      .arg(llmPage.onDeviceHuman)
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
                wrapMode: Text.Wrap
            }

            Label {
                id: legendLabel
                anchors.top: summaryLabel.bottom
                anchors.topMargin: 2
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("Free: %1 RAM · %2 disk")
                      .arg(llmPage.ramHuman).arg(llmPage.diskHuman)
                color: Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
                wrapMode: Text.Wrap
            }

            Label {
                id: updateLabel
                visible: llmPage.updating || llmPage.updateError !== ""
                anchors.top: legendLabel.bottom
                anchors.topMargin: visible ? Theme.paddingSmall : 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: llmPage.updateError !== ""
                      ? qsTr("Catalog update failed: %1").arg(llmPage.updateError)
                      : qsTr("Updating catalog from Hugging Face…")
                color: llmPage.updateError !== "" ? "#e53935" : Theme.highlightColor
                font.pixelSize: Theme.fontSizeExtraSmall
                wrapMode: Text.Wrap
            }
        }

        delegate: ListItem {
            id: item
            width: parent.width
            contentHeight: isHeader
                           ? Theme.itemSizeMedium
                           : Math.max(Theme.itemSizeSmall,
                                      textCol.height
                                      + (dlstate === llmPage.stateDownloading
                                         ? Theme.paddingSmall + 6 : 0)
                                      + Theme.paddingMedium)

            onClicked: llmPage.onRowTap(isHeader, kind, modelId)

            Label {
                id: headerArrow
                visible: isHeader
                width: Theme.itemSizeSmall
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                text: kind === llmPage.secLlm
                      ? (llmPage.llmExpanded ? "▾" : "▸")
                      : (llmPage.dlExpanded ? "▾" : "▸")
                color: parent.highlighted ? Theme.highlightColor : Theme.secondaryColor
                font.pixelSize: Theme.fontSizeSmall
            }

            Label {
                id: headerTitle
                visible: isHeader
                x: Theme.horizontalPageMargin + Theme.itemSizeSmall + Theme.paddingSmall
                width: parent.width - x - Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                text: title + " (" + count + ")"
                color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                font.pixelSize: Theme.fontSizeSmall
                truncationMode: TruncationMode.Fade
            }

            Rectangle {
                id: fitDot
                visible: !isHeader && fit !== ""
                width: 10
                height: 10
                radius: 5
                color: llmPage.fitColor(fit)
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
            }

            Column {
                id: textCol
                visible: !isHeader
                x: Theme.horizontalPageMargin + (fitDot.visible ? 10 + Theme.paddingSmall : 0)
                width: parent.width - 2 * Theme.horizontalPageMargin
                         - Theme.itemSizeMedium - Theme.paddingMedium
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2

                Label {
                    width: parent.width
                    text: name
                    wrapMode: Text.Wrap
                    color: llmPage.isSelected({ model_id: modelId, downloaded: downloaded })
                           ? Theme.highlightColor
                           : (item.highlighted ? Theme.highlightColor : Theme.primaryColor)
                    font.pixelSize: Theme.fontSizeSmall
                }
                Label {
                    width: parent.width
                    text: sub
                    wrapMode: Text.Wrap
                    color: Theme.secondaryColor
                    font.pixelSize: Theme.fontSizeExtraSmall
                }
                Row {
                    visible: dlstate === llmPage.stateDownloading
                    spacing: Theme.paddingMedium
                    width: parent.width
                    BusyIndicator {
                        running: parent.visible
                        size: BusyIndicatorSize.Small
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Label {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - Theme.paddingMedium - Theme.iconSizeSmall
                        text: llmPage.liveInfo(modelId)
                        color: Theme.primaryColor
                        font.pixelSize: Theme.fontSizeSmall
                        truncationMode: TruncationMode.Fade
                    }
                }
            }

            // Right-side quick action (download / cancel / select). The row tap
            // itself opens the detail card.
            MouseArea {
                visible: !isHeader
                anchors.right: parent.right
                anchors.rightMargin: Theme.paddingSmall
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.itemSizeMedium
                height: Theme.itemSizeMedium
                onClicked: llmPage.quickAction(modelId, dlstate, downloaded)

                Label {
                    anchors.centerIn: parent
                    text: llmPage.glyph({ state: dlstate, model_id: modelId,
                                          downloaded: downloaded })
                    color: llmPage.glyphColor({ state: dlstate, model_id: modelId,
                                                downloaded: downloaded })
                    font.pixelSize: Theme.fontSizeSmall
                }
            }

            Rectangle {
                id: bar
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 3
                visible: !isHeader && dlstate === llmPage.stateDownloading
                color: Theme.highlightBackgroundColor
                Rectangle {
                    x: 0
                    y: 0
                    width: parent.width * llmPage.liveRatio(modelId)
                    height: parent.height
                    color: Theme.highlightColor
                }
            }

            menu: ContextMenu {
                MenuItem {
                    text: isHeader
                          ? qsTr("Expand / collapse")
                          : (dlstate === llmPage.stateDownloading
                             ? qsTr("Cancel download")
                             : (downloaded ? qsTr("Remove model") : qsTr("Download")))
                    onClicked: llmPage.onRowMenu(isHeader, kind, modelId, dlstate,
                                                 downloaded, name, item)
                }
            }
        }

        ViewPlaceholder {
            enabled: (llmPage.dlModels.length + llmPage.catModels.length) === 0
            text: llmPage.query !== ""
                  ? qsTr("Nothing matches «%1»").arg(llmPage.query)
                  : qsTr("No models in the catalog")
        }

        PullDownMenu {
            MenuItem {
                text: llmPage.updating
                      ? qsTr("Updating…")
                      : qsTr("Update model catalogs")
                enabled: !llmPage.updating
                onClicked: llmPage.llmCmd(llmPage.cmdCatalogUpdate, "")
            }
        }

        VerticalScrollDecorator {}
    }

    Component.onCompleted: llmPage.refresh()
}
