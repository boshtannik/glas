import QtQuick 2.6
import Sailfish.Silica 1.0

// Thinking (LLM) model management (mirrors harbour-opencode): a downloaded
// section plus the LLM catalog, a language filter, live progress, icon
// actions and deletion with confirmation. Models are shared with opencode
// through the same on-disk store.
Page {
    id: llmPage

    // Snapshot of the bridge LLM store.
    property var llmModels: []
    property var langOptions: [ { id: "auto", name: qsTr("Auto") } ]
    property string chosenLlm: ""
    property string version: ""

    property string filterLang: "auto"
    property bool dlExpanded: true
    property bool llmExpanded: true

    // Protocol constants — must match src/llm.rs.
    readonly property string cmdSelect: "llm_select"
    readonly property string cmdDownload: "llm_download"
    readonly property string cmdDownloadCancel: "llm_download_cancel"
    readonly property string cmdDelete: "llm_delete"
    readonly property string cmdCatalogUpdate: "voice_catalog_update"
    readonly property string stateDownloading: "downloading"
    readonly property string stateError: "error"

    readonly property string secOndevice: "ondevice"
    readonly property string secLlm: "llm"

    // Structural fingerprint (without the progress counter): the list is
    // rebuilt only when selection/state/filter/sections change, not on every
    // 400 ms poll — so scrolling is not reset during a download.
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
            llmPage.llmModels = vo.models || []
            llmPage.chosenLlm = vo.llm || ""
            llmPage.version = vo.version || ""
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
        var arr = llmPage.llmModels || []
        var out = ""
        for (var i = 0; i < arr.length; i++)
            out += arr[i].model_id + "|" + (arr[i].downloaded ? 1 : 0) + "|"
                   + arr[i].state + ";"
        return out + "@" + llmPage.chosenLlm + "$" + llmPage.filterLang
               + "%" + llmPage.llmExpanded + "^" + llmPage.dlExpanded
    }

    function langName(id) {
        if (id === "multilang" || id === "multilingual")
            return qsTr("Multilingual")
        var o = llmPage.langOptions || []
        for (var i = 0; i < o.length; i++)
            if (o[i].id === id)
                return o[i].name || id
        return id
    }

    function filterIndex() {
        var o = llmPage.langOptions || []
        for (var i = 0; i < o.length; i++)
            if (o[i].id === llmPage.filterLang)
                return i
        return 0
    }

    function visible(m) {
        if (llmPage.filterLang === "" || llmPage.filterLang === "auto")
            return true
        if (m.lang_id === "multilang" || m.lang_id === "multilingual")
            return true
        return m.lang_id === llmPage.filterLang
    }

    function isSelected(m) {
        return m.model_id === llmPage.chosenLlm
    }

    function rowSub(m) {
        var s = llmPage.langName(m.lang_id) + " · " + llmPage.humanSize(m.size)
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

    function findModel(id) {
        var arr = llmPage.llmModels || []
        for (var i = 0; i < arr.length; i++)
            if (arr[i].model_id === id)
                return arr[i]
        return null
    }

    function liveInfo(id) {
        var m = llmPage.findModel(id)
        if (!m || m.state !== llmPage.stateDownloading)
            return ""
        var size = parseInt(m.size), done = parseInt(m.done || 0)
        if (!size)
            return ""
        var pct = Math.min(99, Math.round(done / size * 100))
        return qsTr("Downloading… %1% (%2 / %3)")
               .arg(pct)
               .arg(llmPage.humanSize(done))
               .arg(llmPage.humanSize(size))
    }

    function liveRatio(id) {
        var m = llmPage.findModel(id)
        if (!m || m.state !== llmPage.stateDownloading)
            return 0
        var size = parseInt(m.size), done = parseInt(m.done || 0)
        if (!size)
            return 0
        return Math.max(0, Math.min(1, done / size))
    }

    function fillModel() {
        llmModel.clear()
        var arr = llmPage.llmModels || []

        var ondev = llmPage.downloadedArr(arr)
        llmModel.append({ isHeader: true, kind: llmPage.secOndevice,
                          title: qsTr("Downloaded models"),
                          total: ondev.length, done: ondev.length,
                          name: "", sub: "", dlstate: "", modelId: "",
                          downloaded: false })
        if (llmPage.dlExpanded)
            for (var i = 0; i < ondev.length; i++)
                llmPage.appendRow(ondev[i])

        var n = 0
        for (var j = 0; j < arr.length; j++)
            if (!arr[j].downloaded && llmPage.visible(arr[j]))
                n++
        llmModel.append({ isHeader: true, kind: llmPage.secLlm,
                          title: qsTr("Thinking models"),
                          total: n, done: n,
                          name: "", sub: "", dlstate: "", modelId: "",
                          downloaded: false })
        if (llmPage.llmExpanded)
            for (var k = 0; k < arr.length; k++)
                if (!arr[k].downloaded && llmPage.visible(arr[k]))
                    llmPage.appendRow(arr[k])
    }

    function downloadedArr(arr) {
        var out = []
        for (var i = 0; i < arr.length; i++) {
            if (!arr[i].downloaded)
                continue
            var d = {}
            for (var k in arr[i])
                d[k] = arr[i][k]
            out.push(d)
        }
        return out
    }

    function appendRow(m) {
        llmModel.append({ isHeader: false, kind: "", total: 0, done: 0,
                          name: m.name, sub: llmPage.rowSub(m),
                          dlstate: m.state, modelId: m.model_id,
                          downloaded: !!m.downloaded })
    }

    function toggleSection(kind) {
        if (kind === llmPage.secLlm)
            llmPage.llmExpanded = !llmPage.llmExpanded
        else
            llmPage.dlExpanded = !llmPage.dlExpanded
    }

    function onRowTap(isHeader, kind, modelId, dlstate, downloaded) {
        if (isHeader) {
            llmPage.toggleSection(kind)
            return
        }
        if (dlstate === llmPage.stateDownloading)
            llmPage.llmCmd(llmPage.cmdDownloadCancel, modelId)
        else if (downloaded)
            llmPage.llmCmd(llmPage.cmdSelect, modelId)
        else
            llmPage.llmCmd(llmPage.cmdDownload, modelId)
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

    function humanSize(b) {
        if (!b || b <= 0)
            return ""
        var g = b / (1024 * 1024 * 1024)
        if (g >= 1)
            return g.toFixed(1) + " GB"
        var m = b / (1024 * 1024)
        if (m >= 1)
            return Math.round(m) + " MB"
        return Math.round(b / 1024) + " KB"
    }

    function downloadedCount() {
        var arr = llmPage.llmModels || []
        var c = 0
        for (var i = 0; i < arr.length; i++)
            if (arr[i].downloaded)
                c++
        return c
    }

    function downloadedBytes() {
        var arr = llmPage.llmModels || []
        var b = 0
        for (var i = 0; i < arr.length; i++)
            if (arr[i].downloaded)
                b += parseInt(arr[i].size || 0)
        return b
    }

    RemorseItem { id: remorse }

    SilicaListView {
        id: list
        anchors.fill: parent
        model: llmModel

        header: Item {
            width: parent.width
            height: filterCombo.height + summaryLabel.height + Theme.paddingMedium

            ComboBox {
                id: filterCombo
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
                }
            }

            Label {
                id: summaryLabel
                anchors.top: filterCombo.bottom
                anchors.topMargin: Theme.paddingSmall
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("On device: %1 of %2 models, %3")
                      .arg(llmPage.downloadedCount())
                      .arg((llmPage.llmModels || []).length)
                      .arg(llmPage.humanSize(llmPage.downloadedBytes()))
                color: Theme.secondaryColor
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

            onClicked: llmPage.onRowTap(isHeader, kind, modelId, dlstate,
                                        downloaded)

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
                text: (total === done) ? title + " (" + done + ")"
                                       : title + " (" + done + "/" + total + ")"
                color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                font.pixelSize: Theme.fontSizeSmall
                truncationMode: TruncationMode.Fade
            }

            Column {
                id: textCol
                visible: !isHeader
                x: Theme.horizontalPageMargin
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

            Label {
                id: iconLbl
                visible: !isHeader
                anchors.right: parent.right
                anchors.rightMargin: Theme.paddingSmall
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.itemSizeMedium
                horizontalAlignment: Text.AlignHCenter
                text: llmPage.glyph({ state: dlstate, model_id: modelId,
                                     downloaded: downloaded })
                color: llmPage.glyphColor({ state: dlstate, model_id: modelId,
                                            downloaded: downloaded })
                font.pixelSize: Theme.fontSizeSmall
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
            enabled: list.count === 0
            text: qsTr("No models for the selected language")
        }

        PullDownMenu {
            MenuItem {
                text: qsTr("Update model catalogs")
                onClicked: llmPage.llmCmd(llmPage.cmdCatalogUpdate, "")
            }
        }

        VerticalScrollDecorator {}
    }

    Component.onCompleted: llmPage.refresh()
}