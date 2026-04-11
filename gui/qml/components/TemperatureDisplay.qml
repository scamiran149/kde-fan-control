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
import org.kde.kirigami as Kirigami

Controls.Label {
    id: tempDisplay

    property int millidegrees: 0
    property bool showUnit: true
    property bool showNoSource: false

    text: {
        if (showNoSource || millidegrees <= 0) {
            return i18n("No control source")
        }
        var celsius = millidegrees / 1000.0
        return celsius.toFixed(1) + (showUnit ? " °C" : "")
    }

    font.weight: Font.Normal
    color: {
        if (showNoSource || millidegrees <= 0) {
            return Kirigami.Theme.disabledTextColor
        }
        return Kirigami.Theme.textColor
    }
}