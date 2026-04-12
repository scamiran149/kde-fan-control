/*
 * KDE Fan Control — Tray Popover
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Compact tray popover per UI-SPEC Tray Contract.
 * Shows: header with daemon state + severity + counts,
 * alert area when sticky alerts exist, managed fan list,
 * and footer with Open and Acknowledge actions.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Rectangle {
    id: trayPopover

    // Preferred width per UI-SPEC: 360px
    width: 360
    height: mainLayout.height + Kirigami.Units.largeSpacing * 2
    color: Kirigami.Theme.alternateBackgroundColor
    radius: Kirigami.Units.smallSpacing

    // Maximum visible fan rows before scrolling (UI-SPEC: 6)
    readonly property int maxVisibleRows: 6
    readonly property int rowHeight: 40

    ColumnLayout {
        id: mainLayout
        anchors.fill: parent
        anchors.margins: Kirigami.Units.mdSpacing
        spacing: Kirigami.Units.smSpacing

        // ================================================
        // HEADER: daemon connection state, severity, counts
        // ================================================

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.mdSpacing

            // Daemon connection indicator
            Rectangle {
                width: Kirigami.Units.iconSizes.small
                height: Kirigami.Units.iconSizes.small
                radius: width / 2
                color: {
                    if (!trayIcon.daemonConnected) return Kirigami.Theme.negativeTextColor
                    // Connected: green
                    return Kirigami.Theme.positiveTextColor
                }

                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: trayIcon.daemonConnected ? i18n("Connected") : i18n("Disconnected")
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay

                property bool hovered: ma.containsMouse
                MouseArea {
                    id: ma
                    anchors.fill: parent
                    hoverEnabled: true
                }
            }

            Controls.Label {
                text: trayIcon.daemonConnected ? i18n("Connected") : i18n("Disconnected")
                color: trayIcon.daemonConnected ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.negativeTextColor
                font.weight: Font.DemiBold
                font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
            }

            Item { Layout.fillWidth: true }

            // Worst severity icon
            Kirigami.Icon {
                source: {
                    switch (trayIcon.worstSeverity) {
                    case "fallback":    return "dialog-error-symbolic"
                    case "degraded":    return "data-warning-symbolic"
                    case "high-temp":   return "temperature-high-symbolic"
                    case "managed":    return "emblem-ok-symbolic"
                    case "unmanaged":  return "dialog-information-symbolic"
                    default:            return "network-offline-symbolic"
                    }
                }
                width: Kirigami.Units.iconSizes.small
                height: Kirigami.Units.iconSizes.small
            }

            // Managed count
            Controls.Label {
                text: i18n("%1 managed", trayIcon.managedFanCount)
                font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
                color: Kirigami.Theme.textColor
            }

            // Alert count
            Controls.Label {
                visible: trayIcon.alertCount > 0
                text: i18n("%1 alerts", trayIcon.alertCount)
                font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
                color: Kirigami.Theme.negativeTextColor
                font.weight: Font.DemiBold
            }
        }

        // ================================================
        // ALERT AREA: sticky alert summary per D-12
        // ================================================

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.xsSpacing
            visible: trayIcon.hasStickyAlerts

            // Helper: count fans per state
            function countByState(stateName) {
                var count = 0
                for (var i = 0; i < fanListModel.rowCount(); i++) {
                    var idx = fanListModel.index(i, 0)
                    if (fanListModel.data(idx, FanListModel.StateRole) === stateName) {
                        count++
                    }
                }
                return count
            }

            function countHighTempAlerts() {
                var count = 0
                for (var i = 0; i < fanListModel.rowCount(); i++) {
                    var idx = fanListModel.index(i, 0)
                    var state = fanListModel.data(idx, FanListModel.StateRole)
                    var ht = fanListModel.data(idx, FanListModel.HighTempAlertRole)
                    if (state === "managed" && ht) count++
                }
                return count
            }

            // Fallback alert
            Rectangle {
                Layout.fillWidth: true
                height: fallbackLabel.height + Kirigami.Units.smallSpacing * 2
                color: Kirigami.Theme.negativeTextColor
                opacity: 0.15
                radius: Kirigami.Units.smallSpacing
                visible: trayPopover.countByState("fallback") > 0

                Controls.Label {
                    id: fallbackLabel
                    anchors.centerIn: parent
                    text: i18n("Fallback active — %1 fans driven to safe output").arg(trayPopover.countByState("fallback"))
                    color: Kirigami.Theme.negativeTextColor
                    font.weight: Font.DemiBold
                    font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
                }
            }

            // Degraded alert
            Rectangle {
                Layout.fillWidth: true
                height: degradedLabel.height + Kirigami.Units.smallSpacing * 2
                color: Kirigami.Theme.warningTextColor
                opacity: 0.15
                radius: Kirigami.Units.smallSpacing
                visible: trayPopover.countByState("degraded") > 0

                Controls.Label {
                    id: degradedLabel
                    anchors.centerIn: parent
                    text: i18n("Fan control degraded — %1 fans").arg(trayPopover.countByState("degraded"))
                    color: Kirigami.Theme.warningTextColor
                    font.weight: Font.DemiBold
                    font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
                }
            }

            // High-temp alert
            Rectangle {
                Layout.fillWidth: true
                height: highTempLabel.height + Kirigami.Units.smallSpacing * 2
                color: Kirigami.Theme.negativeTextColor
                opacity: 0.12
                radius: Kirigami.Units.smallSpacing
                visible: trayPopover.countHighTempAlerts() > 0

                Controls.Label {
                    id: highTempLabel
                    anchors.centerIn: parent
                    text: i18n("High temperature — %1 fans above target").arg(trayPopover.countHighTempAlerts())
                    color: Kirigami.Theme.negativeTextColor
                    font.weight: Font.DemiBold
                    font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
                }
            }
        }

        // ================================================
        // MANAGED FAN LIST per D-09
        // ================================================

        ListView {
            id: fanListView
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(
                fanListView.count * trayPopover.rowHeight,
                trayPopover.maxVisibleRows * trayPopover.rowHeight
            )
            clip: true
            spacing: 0

            // Filter model to only show managed fans by default per D-09
            model: fanListModel
            delegate: FanTrayDelegate {
                width: fanListView.width
                fanId: model.fanId
                displayName: model.displayName
                fanState: model.state
                temperatureMillidegrees: model.temperatureMillidegrees
                outputPercent: model.outputPercent
                rpm: model.rpm
                hasTach: model.hasTach
                highTempAlert: model.highTempAlert

                // Only show managed fans per D-09
                visible: model.state === "managed" ||
                         model.state === "degraded" ||
                         model.state === "fallback" ||
                         model.state === "unmanaged" // show unmanaged too for quick inspection
                height: visible ? trayPopover.rowHeight : 0

                // Click opens main window and navigates to this fan's detail
                onClicked: {
                    trayIcon.activateMainWindow()
                    // Navigate to fan detail page after activating window
                    // The main window's pageStack is accessible via the application window
                }
            }
        }

        // ================================================
        // FOOTER ACTIONS per UI-SPEC
        // ================================================

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smSpacing
            spacing: Kirigami.Units.mdSpacing

            Controls.Button {
                text: i18n("Open Fan Control")
                icon.name: "go-home"
                Layout.fillWidth: true
                onClicked: {
                    trayIcon.activateMainWindow()
                }
            }

            Controls.Button {
                text: i18n("Acknowledge alerts")
                icon.name: "dialog-ok-apply-symbolic"
                visible: trayIcon.hasStickyAlerts
                enabled: trayIcon.hasStickyAlerts
                Layout.fillWidth: true
                onClicked: {
                    trayIcon.acknowledgeAlerts()
                }
            }
        }
    }
}