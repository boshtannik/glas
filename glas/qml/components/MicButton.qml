import QtQuick 2.6
import Sailfish.Silica 1.0

IconButton {
    id: control

    // A big push-to-talk button (the previous project used itemSizeMedium).
    property int buttonSize: Theme.itemSizeMedium * 3

    width: buttonSize
    height: buttonSize
    icon.source: "image://theme/icon-m-mic"
    icon.sourceSize.width: buttonSize
    icon.sourceSize.height: buttonSize
}
