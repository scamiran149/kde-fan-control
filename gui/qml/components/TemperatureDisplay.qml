/*
 * KDE Fan Control — Temperature Display Component
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Compact temperature display converting millidegrees to Celsius with 1 decimal.
 * Shows "No control source" for unmanaged/unsupported fans.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.Label {
    id: tempDisplay

    property int millidegrees: 0
    property bool showUnit: true
    property bool showNoSource: false
    property bool fixedWidth: false

    text: {
        if (showNoSource) {
            return i18n("No control source")
        }
        if (millidegrees <= 0) {
            return i18n("No live reading")
        }
        var celsius = millidegrees / 1000.0
        return celsius.toFixed(1) + (showUnit ? " °C" : "")
    }

    font.weight: Font.Normal
    font.family: fixedWidth ? "monospace" : Kirigami.Theme.defaultFont.family
    font.pixelSize: fixedWidth ? Kirigami.Theme.smallFont.pixelSize : Kirigami.Theme.defaultFont.pixelSize
    color: {
        if (showNoSource || millidegrees <= 0) {
            return Kirigami.Theme.disabledTextColor
        }
        return Kirigami.Theme.textColor
    }

    Layout.minimumWidth: fixedWidth ? 80 : -1
    Layout.preferredWidth: fixedWidth ? 80 : -1
    horizontalAlignment: fixedWidth ? Text.AlignRight : Text.AlignLeft
}
