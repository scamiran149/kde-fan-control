/*
 * KDE Fan Control — Fan Tray Delegate
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Compact tray fan row per UI-SPEC/10:
 * - Fan name + state icon on left
 * - Temperature (°C, 1 decimal) on right
 * - Output percent or RPM on far right
 * - Row height minimum 40px per UI-SPEC
 * - Clicking opens main window to fan detail page
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Item {
    id: fanTrayDelegate

    // Properties bound to FanListModel roles
    property string fanId: ""
    property string displayName: ""
    property string fanState: "unmanaged"
    property int temperatureMillidegrees: 0
    property double outputPercent: -1.0
    property int rpm: 0
    property bool hasTach: false
    property bool highTempAlert: false

    // Row height per UI-SPEC: 40px minimum
    height: 40

    // Helper to format temperature
    function formatTemp(millideg) {
        if (millideg <= 0) return i18n("N/A")
        return (millideg / 1000.0).toFixed(1) + " °C"
    }

    // Helper to format output
    function formatOutput(percent, hasRpm, rpm) {
        if (percent < 0) return i18n("No control")
        if (hasRpm && rpm > 0) return Math.round(percent) + "%"
        return Math.round(percent) + "%"
    }

    // Click handler: open main window to this fan's detail page
    signal clicked()

    // Background highlight on hover
    Rectangle {
        anchors.fill: parent
        color: mouseArea.containsMouse ? Kirigami.Theme.highlightColor : "transparent"
        opacity: mouseArea.containsMouse ? 0.1 : 0
        radius: Kirigami.Units.smallSpacing

        Behavior on opacity {
            NumberAnimation { duration: 150 }
        }
    }

    RowLayout {
        id: rowLayout
        anchors.fill: parent
        anchors.leftMargin: Kirigami.Units.mediumSpacing
        anchors.rightMargin: Kirigami.Units.mediumSpacing
        spacing: Kirigami.Units.smallSpacing

        // State icon (compact version of StateBadge)
        Kirigami.Icon {
            source: {
                switch (fanTrayDelegate.fanState) {
                case "managed":    return fanTrayDelegate.highTempAlert ? "temperature-high-symbolic" : "emblem-ok-symbolic"
                case "unmanaged":  return "dialog-information-symbolic"
                case "degraded":   return "data-warning-symbolic"
                case "fallback":   return "dialog-error-symbolic"
                case "partial":    return "dialog-warning-symbolic"
                case "unavailable": return "emblem-unavailable-symbolic"
                default:           return "dialog-question-symbolic"
                }
            }
            width: Kirigami.Units.iconSizes.small
            height: Kirigami.Units.iconSizes.small
            Layout.alignment: Qt.AlignVCenter
        }

        // Fan display name
        Controls.Label {
            text: fanTrayDelegate.displayName
            font.weight: Font.Medium
            font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
            Layout.fillWidth: true
            elide: Text.ElideRight
            Layout.alignment: Qt.AlignVCenter
        }

        // Temperature (right side)
        Controls.Label {
            text: fanTrayDelegate.formatTemp(fanTrayDelegate.temperatureMillidegrees)
            font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
            color: Kirigami.Theme.textColor
            Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
        }

        // Output percent (far right)
        Controls.Label {
            text: {
                if (fanTrayDelegate.fanState === "managed") {
                    return fanTrayDelegate.formatOutput(
                        fanTrayDelegate.outputPercent,
                        fanTrayDelegate.hasTach,
                        fanTrayDelegate.rpm
                    )
                }
                // Non-managed fans don't show output
                return ""
            }
            font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
            color: Kirigami.Theme.disabledTextColor
            visible: fanTrayDelegate.fanState === "managed"
            Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
            Layout.minimumWidth: visible ? 40 : 0
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        onClicked: fanTrayDelegate.clicked()

        Controls.ToolTip.visible: mouseArea.containsMouse
        Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
        Controls.ToolTip.text: {
            var state = fanTrayDelegate.fanState
            var tip = fanTrayDelegate.displayName + " — "
            switch (state) {
            case "managed": tip += i18n("Managed"); break
            case "unmanaged": tip += i18n("Unmanaged"); break
            case "degraded": tip += i18n("Degraded"); break
            case "fallback": tip += i18n("Fallback"); break
            default: tip += i18n("Unknown"); break
            }
            if (fanTrayDelegate.highTempAlert) {
                tip += i18n(" (High temp alert)")
            }
            return tip
        }
    }
}
