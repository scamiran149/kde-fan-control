/*
 * KDE Fan Control — State Badge Component
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Traffic-light severity badge showing fan state with icon, color, and text.
 * Per UI-SPEC: managed=positive, unmanaged=neutral, degraded=warning,
 * fallback=negative, partial=warning, unavailable=disabled.
 * High-temp alert is overlaid as a separate negative pill.
 *
 * Supports optional overrides from OverviewFanRow for server-computed
 * state text, icon, and color (bypassing local switch logic).
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

    property string stateTextOverride: ""
    property string stateIconOverride: ""
    property string stateColorOverride: ""

    // State badge pill
    Rectangle {
        radius: Kirigami.Units.smallSpacing
        height: 24
        width: badgeLabel.width + Kirigami.Units.smallSpacing * 4
        color: stateBadge._badgeColor
        opacity: 0.2

        Controls.Label {
            id: badgeLabel
            anchors.centerIn: parent
            text: stateBadge._badgeText
            font.weight: Font.DemiBold
            font.pixelSize: Kirigami.Theme.smallFont.pixelSize
        }

        Kirigami.Icon {
            source: stateBadge._badgeIcon
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
        width: alertLabel.width + Kirigami.Units.smallSpacing * 4
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

    // --- Computed properties via functions (QML-compatible) ---

    readonly property string _badgeText: {
        if (stateTextOverride.length > 0) return stateTextOverride
        switch (fanState) {
        case "managed":     return i18n("Managed")
        case "unmanaged":   return i18n("Unmanaged")
        case "degraded":    return i18n("Degraded")
        case "fallback":    return i18n("Fallback")
        case "partial":     return i18n("Partial")
        case "unavailable": return i18n("Unsupported")
        default:            return i18n("Unknown")
        }
    }

    readonly property string _badgeIcon: {
        if (stateIconOverride.length > 0) return stateIconOverride
        switch (fanState) {
        case "managed":     return "emblem-ok-symbolic"
        case "unmanaged":   return "dialog-information-symbolic"
        case "degraded":    return "data-warning-symbolic"
        case "fallback":    return "dialog-error-symbolic"
        case "partial":     return "dialog-warning-symbolic"
        case "unavailable": return "emblem-unavailable-symbolic"
        default:            return "dialog-question-symbolic"
        }
    }

    readonly property color _badgeColor: {
        if (stateColorOverride.length > 0 && stateColorOverride.charAt(0) === "#")
            return stateColorOverride
        if (stateColorOverride === "positive")   return Kirigami.Theme.positiveTextColor
        if (stateColorOverride === "neutral")    return Kirigami.Theme.neutralTextColor
        if (stateColorOverride === "negative")   return Kirigami.Theme.negativeTextColor
        if (stateColorOverride === "warning")    return Kirigami.Theme.neutralTextColor
        if (stateColorOverride === "disabled")   return Kirigami.Theme.disabledTextColor
        if (stateColorOverride.length > 0)       return Kirigami.Theme.disabledTextColor
        switch (fanState) {
        case "managed":     return Kirigami.Theme.positiveTextColor
        case "unmanaged":   return Kirigami.Theme.neutralTextColor
        case "degraded":    return Kirigami.Theme.neutralTextColor
        case "fallback":    return Kirigami.Theme.negativeTextColor
        case "partial":     return Kirigami.Theme.neutralTextColor
        case "unavailable": return Kirigami.Theme.disabledTextColor
        default:            return Kirigami.Theme.disabledTextColor
        }
    }
}
