import QtQuick 2.6
import Sailfish.Silica 1.0

// Voice model management (mirrors harbour-opencode): two collapsible sections
// (recognition / synthesis), a language filter, live progress, icon actions
// and deletion with confirmation. Models are shared with opencode through the
// same on-disk store.
Page {
    id: vmPage

    // Snapshot of the bridge voice store.
    property var voiceModels: []
    property var voiceLangOptions: [ { id: "auto", name: qsTr("Auto (detect)") } ]
    property string chosenStt: ""
    property string chosenTts: ""
    property string version: ""

    property string filterLang: "auto"
    property bool dlExpanded: true
    property bool recExpanded: true
    property bool ttsExpanded: true

    // Protocol constants — must match src/voice.rs.
    readonly property string cmdLang: "voice_lang"
    readonly property string cmdSelectStt: "voice_select_stt"
    readonly property string cmdSelectTts: "voice_select_tts"
    readonly property string cmdDownload: "voice_download"
    readonly property string cmdDownloadCancel: "voice_download_cancel"
    readonly property string cmdDelete: "voice_delete"
    readonly property string cmdCatalogUpdate: "voice_catalog_update"
    readonly property string engineStt: "stt_whisper"
    readonly property string engineTts: "tts_piper"
    readonly property string stateDownloading: "downloading"
    readonly property string stateError: "error"

    readonly property string secOndevice: "ondevice"
    readonly property string secRecognition: "recognition"
    readonly property string secSynthesis: "synthesis"

    // Structural fingerprint (without the progress counter): the list is
    // rebuilt only when selection/state/filter/sections change, not on every
    // 400 ms poll — so scrolling is not reset during a download.
    property string voiceFp: fingerprint()
    onVoiceFpChanged: fillModel()

    ListModel { id: voiceModel }

    Timer {
        interval: 400
        running: vmPage.status === PageStatus.Active
        repeat: true
        onTriggered: vmPage.refresh()
    }

    function refresh() {
        if (typeof bridge === "undefined")
            return
        var s = bridge.voice_status_json()
        if (!s || s === "")
            return
        try {
            var vo = JSON.parse(s)
            vmPage.voiceModels = vo.models || []
            vmPage.chosenStt = vo.stt || ""
            vmPage.chosenTts = vo.tts || ""
            vmPage.version = vo.version || ""
            if (vo.langs && vo.langs.length) {
                var opts = [{ id: "auto", name: qsTr("Auto (detect)") }]
                for (var i = 0; i < vo.langs.length; i++) {
                    var L = vo.langs[i]
                    opts.push({ id: L.id, name: L.name || L.id })
                }
                vmPage.voiceLangOptions = opts
            }
        } catch (e) {
            console.log("voice parse error: " + e)
        }
    }

    function voiceCmd(cmd, val) {
        if (typeof bridge !== "undefined")
            bridge.voice_command(cmd, val === undefined ? "" : val)
    }

    function fingerprint() {
        var arr = vmPage.voiceModels || []
        var out = ""
        for (var i = 0; i < arr.length; i++)
            out += arr[i].model_id + "|" + (arr[i].downloaded ? 1 : 0) + "|"
                   + arr[i].state + ";"
        return out + "@" + vmPage.chosenStt + "#" + vmPage.chosenTts
               + "$" + vmPage.filterLang + "%" + vmPage.recExpanded
               + "!" + vmPage.ttsExpanded + "^" + vmPage.dlExpanded
    }

    function langName(id) {
        if (id === "multilang" || id === "multilingual")
            return qsTr("Multilingual")
        var o = vmPage.voiceLangOptions || []
        for (var i = 0; i < o.length; i++)
            if (o[i].id === id)
                return o[i].name || id
        return id
    }

    function filterIndex() {
        var o = vmPage.voiceLangOptions || []
        for (var i = 0; i < o.length; i++)
            if (o[i].id === vmPage.filterLang)
                return i
        return 0
    }

    function visible(m) {
        if (vmPage.filterLang === "" || vmPage.filterLang === "auto")
            return true
        if (m.lang_id === "multilang" || m.lang_id === "multilingual")
            return true
        return m.lang_id === vmPage.filterLang
    }

    function storeRows(m, engine) {
        if (m.hidden || m.engine !== engine || !vmPage.visible(m))
            return false
        if (vmPage.filterLang === "" || vmPage.filterLang === "auto")
            return !m.downloaded
        return true
    }

    function isSelected(m) {
        return m.model_id === vmPage.chosenStt
               || m.model_id === vmPage.chosenTts
    }

    function rowSub(m) {
        var s = vmPage.langName(m.lang_id) + " · " + vmPage.humanSize(m.size)
        if (m.state === vmPage.stateError)
            s += " · " + qsTr("download failed")
        else if (m.downloaded)
            s += " · " + qsTr("downloaded")
        return s
    }

    function glyph(m) {
        if (m.state === vmPage.stateDownloading)
            return "✕"
        if (m.state === vmPage.stateError)
            return "!"
        if (vmPage.isSelected(m))
            return "✓"
        if (m.downloaded)
            return "→"
        return "↓"
    }

    function glyphColor(m) {
        if (m.state === vmPage.stateDownloading || m.state === vmPage.stateError)
            return Theme.secondaryColor
        if (vmPage.isSelected(m))
            return Theme.highlightColor
        return Theme.primaryColor
    }

    function findModel(id) {
        var arr = vmPage.voiceModels || []
        for (var i = 0; i < arr.length; i++)
            if (arr[i].model_id === id)
                return arr[i]
        return null
    }

    function liveInfo(id) {
        var m = vmPage.findModel(id)
        if (!m || m.state !== vmPage.stateDownloading)
            return ""
        var size = parseInt(m.size), done = parseInt(m.done || 0)
        if (!size)
            return ""
        var pct = Math.min(99, Math.round(done / size * 100))
        return qsTr("Downloading… %1% (%2 / %3)")
               .arg(pct)
               .arg(vmPage.humanSize(done))
               .arg(vmPage.humanSize(size))
    }

    function liveRatio(id) {
        var m = vmPage.findModel(id)
        if (!m || m.state !== vmPage.stateDownloading)
            return 0
        var size = parseInt(m.size), done = parseInt(m.done || 0)
        if (!size)
            return 0
        return Math.max(0, Math.min(1, done / size))
    }

    function fillModel() {
        voiceModel.clear()
        var arr = vmPage.voiceModels || []

        var ondev = vmPage.downloadedArr(arr)
        voiceModel.append({ isHeader: true, kind: vmPage.secOndevice,
                            title: qsTr("Downloaded models"),
                            total: ondev.length, done: ondev.length,
                            name: "", sub: "", dlstate: "", modelId: "",
                            engine: "", downloaded: false })
        if (vmPage.dlExpanded)
            for (var i = 0; i < ondev.length; i++)
                vmPage.appendRow(ondev[i])

        vmPage.appendStore(arr, vmPage.engineStt, qsTr("Recognition"),
                           vmPage.secRecognition, vmPage.recExpanded)
        vmPage.appendStore(arr, vmPage.engineTts, qsTr("Synthesis"),
                           vmPage.secSynthesis, vmPage.ttsExpanded)
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

    function appendStore(arr, engine, title, kind, expanded) {
        var n = 0
        for (var i = 0; i < arr.length; i++)
            if (vmPage.storeRows(arr[i], engine))
                n++
        voiceModel.append({ isHeader: true, kind: kind,
                            title: title, total: n, done: n,
                            name: "", sub: "", dlstate: "", modelId: "",
                            engine: "", downloaded: false })
        if (expanded)
            for (var j = 0; j < arr.length; j++)
                if (vmPage.storeRows(arr[j], engine))
                    vmPage.appendRow(arr[j])
    }

    function appendRow(m) {
        voiceModel.append({ isHeader: false, kind: m.engine,
                            title: "", total: 0, done: 0,
                            name: m.name, sub: vmPage.rowSub(m),
                            dlstate: m.state, modelId: m.model_id,
                            engine: m.engine, downloaded: !!m.downloaded })
    }

    function toggleSection(kind) {
        if (kind === vmPage.secRecognition)
            vmPage.recExpanded = !vmPage.recExpanded
        else if (kind === vmPage.secSynthesis)
            vmPage.ttsExpanded = !vmPage.ttsExpanded
        else
            vmPage.dlExpanded = !vmPage.dlExpanded
    }

    function onRowTap(isHeader, kind, modelId, engine, dlstate, downloaded) {
        if (isHeader) {
            vmPage.toggleSection(kind)
            return
        }
        if (dlstate === vmPage.stateDownloading)
            vmPage.voiceCmd(vmPage.cmdDownloadCancel, modelId)
        else if (downloaded)
            vmPage.voiceCmd(engine === vmPage.engineStt
                            ? vmPage.cmdSelectStt : vmPage.cmdSelectTts, modelId)
        else
            vmPage.voiceCmd(vmPage.cmdDownload, modelId)
    }

    function onRowMenu(isHeader, kind, modelId, dlstate, downloaded, name, wdg) {
        if (isHeader) {
            vmPage.toggleSection(kind)
            return
        }
        if (dlstate === vmPage.stateDownloading)
            vmPage.voiceCmd(vmPage.cmdDownloadCancel, modelId)
        else if (downloaded)
            remorse.execute(wdg, qsTr("Removing «%1»…").arg(name), function() {
                vmPage.voiceCmd(vmPage.cmdDelete, modelId)
            })
        else
            vmPage.voiceCmd(vmPage.cmdDownload, modelId)
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
        var arr = vmPage.voiceModels || []
        var c = 0
        for (var i = 0; i < arr.length; i++)
            if (arr[i].downloaded)
                c++
        return c
    }

    function downloadedBytes() {
        var arr = vmPage.voiceModels || []
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
        model: voiceModel

        header: Item {
            width: parent.width
            height: filterCombo.height + summaryLabel.height + Theme.paddingMedium

            ComboBox {
                id: filterCombo
                width: parent.width
                label: qsTr("Language")
                currentIndex: vmPage.filterIndex()
                menu: ContextMenu {
                    Repeater {
                        model: vmPage.voiceLangOptions
                        MenuItem { text: modelData.name }
                    }
                }
                onCurrentIndexChanged: {
                    var o = vmPage.voiceLangOptions
                    if (currentIndex < 0 || currentIndex >= o.length)
                        return
                    vmPage.filterLang = o[currentIndex].id
                }
            }

            Label {
                id: summaryLabel
                anchors.top: filterCombo.bottom
                anchors.topMargin: Theme.paddingSmall
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                text: qsTr("On device: %1 of %2 models, %3")
                      .arg(vmPage.downloadedCount())
                      .arg((vmPage.voiceModels || []).length)
                      .arg(vmPage.humanSize(vmPage.downloadedBytes()))
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
                                      + (dlstate === vmPage.stateDownloading
                                         ? Theme.paddingSmall + 6 : 0)
                                      + Theme.paddingMedium)

            onClicked: vmPage.onRowTap(isHeader, kind, modelId, engine,
                                       dlstate, downloaded)

            Label {
                id: headerArrow
                visible: isHeader
                width: Theme.itemSizeSmall
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                text: kind === vmPage.secRecognition
                      ? (vmPage.recExpanded ? "▾" : "▸")
                      : kind === vmPage.secSynthesis
                        ? (vmPage.ttsExpanded ? "▾" : "▸")
                        : (vmPage.dlExpanded ? "▾" : "▸")
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
                    color: vmPage.isSelected({ model_id: modelId, downloaded: downloaded })
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
                    visible: dlstate === vmPage.stateDownloading
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
                        text: vmPage.liveInfo(modelId)
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
                text: vmPage.glyph({ state: dlstate, model_id: modelId,
                                     downloaded: downloaded })
                color: vmPage.glyphColor({ state: dlstate, model_id: modelId,
                                           downloaded: downloaded })
                font.pixelSize: Theme.fontSizeSmall
            }

            Rectangle {
                id: bar
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 3
                visible: !isHeader && dlstate === vmPage.stateDownloading
                color: Theme.highlightBackgroundColor
                Rectangle {
                    x: 0
                    y: 0
                    width: parent.width * vmPage.liveRatio(modelId)
                    height: parent.height
                    color: Theme.highlightColor
                }
            }

            menu: ContextMenu {
                MenuItem {
                    text: isHeader
                          ? qsTr("Expand / collapse")
                          : (dlstate === vmPage.stateDownloading
                             ? qsTr("Cancel download")
                             : (downloaded ? qsTr("Remove model") : qsTr("Download")))
                    onClicked: vmPage.onRowMenu(isHeader, kind, modelId, dlstate,
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
                onClicked: vmPage.voiceCmd(vmPage.cmdCatalogUpdate, "")
            }
        }

        VerticalScrollDecorator {}
    }

    Component.onCompleted: vmPage.refresh()
}
