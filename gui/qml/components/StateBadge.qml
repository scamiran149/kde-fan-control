/*
 * KDE Fan Control — State Badge Component
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Traffic-light severity badge showing fan state with icon, color, and text.
 * Per UI-SPEC: managed=positive, unmanaged=neutral, degraded=warning,
 * fallback=negative, partial=warning, unavailable=disabled.
 * High-temp alert is overlaid as a separate negative pill.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

RowLayout {
    id: stateBadge
    spacing: Kirigami.Units.smallSpacing

    property string fanState: "unmanaged"
    property bool highTempAlert: false

    // State badge pill
    Rectangle {
        radius: Kirigami.Units.smallSpacing
        height: 24
        width: badgeLabel.width + Kirigami.Units.smallSpacing * 2
        color: {
            switch (stateBadge.fanState) {
            case "managed":   return Kirigami.Theme.positiveTextColor
            case "unmanaged":  return Kirigami.Theme.neutralTextColor
            case "degraded":   return Kirigami.Theme.neutralTextColor
            case "fallback":   return Kirigami.Theme.negativeTextColor
            case "partial":    return Kirigami.Theme.neutralTextColor
            case "unavailable": return Kirigami.Theme.disabledTextColor
            default:           return Kirigami.Theme.disabledTextColor
            }
        }
        opacity: 0.2

        Controls.Label {
            id: badgeLabel
            anchors.centerIn: parent
            text: {
                switch (stateBadge.fanState) {
                case "managed":    return i18n("Managed")
                case "unmanaged":  return i18n("Unmanaged")
                case "degraded":   return i18n("Degraded")
                case "fallback":   return i18n("Fallback")
                case "partial":    return i18n("Partial")
                case "unavailable": return i18n("Unsupported")
                default:           return i18n("Unknown")
                }
            }
            font.weight: Font.DemiBold
            font.pixelSize: Kirigami.Theme.smallFont.pixelSize
        }

        Kirigami.Icon {
            source: {
                switch (stateBadge.fanState) {
                case "managed":    return "emblem-ok-symbolic"
                case "unmanaged":  return "dialog-information-symbolic"
                case "degraded":    return "data-warning-symbolic"
                case "fallback":    return "dialog-error-symbolic"
                case "partial":     return "dialog-warning-symbolic"
                case "unavailable": return "emblem-unavailable-symbolic"
                default:           return "dialog-question-symbolic"
                }
            }
            width: Kirigami.Units.iconSizes.small
            height: Kirigami.Units.iconSizes.small
            anchors.left: parent.left
            anchors.leftMargin: Kirigami.Units.smallSpacing
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    // High-temp alert overlay pill (per UI-SPEC: separate pill layered on top of base state)
    Rectangle {
        radius: Kirigami.Units.smallSpacing
        height: 24
        width: alertLabel.width + Kirigami.Units.smallSpacing * 2
        color: Kirigami.Theme.negativeTextColor
        opacity: 0.15
        visible: stateBadge.highTempAlert

        Controls.Label {
            id: alertLabel
            anchors.centerIn: parent
            text: i18n("High temp")
            font.weight: Font.DemiBold
            font.pixelSize: Kirigami.Theme.smallFont.pixelSize
            color: Kirigami.Theme.negativeTextColor
            visible: stateBadge.highTempAlert
        }

        Kirigami.Icon {
            source: "temperature-high-symbolic"
            width: Kirigami.Units.iconSizes.small
            height: Kirigami.Units.iconSizes.small
            anchors.left: parent.left
            anchors.leftMargin: Kirigami.Units.smallSpacing
            anchors.verticalCenter: parent.verticalCenter
            visible: stateBadge.highTempAlert
        }
    }
}
