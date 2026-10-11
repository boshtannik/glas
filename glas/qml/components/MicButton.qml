import QtQuick 2.6
import Sailfish.Silica 1.0

IconButton {
    id: control

    icon.source: "image://theme/icon-l-mic"
    icon.width: Theme.iconSizeLarge * 2
    icon.height: Theme.iconSizeLarge * 2
    width: icon.width + 2 * Theme.paddingLarge
    height: width
}
