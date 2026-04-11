/*
 * KDE Fan Control — Overview Page
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Shows all fans with severity banners and live metrics.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Kirigami.ScrollablePage {
    id: overviewPage
    title: i18n("Fan Control")

    // Helper functions to check for presence of specific states
    function hasFansWithState(stateName) {
        for (var i = 0; i < fanListModel.rowCount(); i++) {
            var idx = fanListModel.index(i, 0);
            if (fanListModel.data(idx, FanListModel.StateRole) === stateName) {
                return true;
            }
        }
        return false;
    }

    ColumnLayout {
        id: mainLayout
        spacing: Kirigami.Units.mdSpacing

        // -- Severity banners (mutually exclusive in display order: fallback > degraded > disconnected) --

        // Fallback banner
        Kirigami.InlineMessage {
            id: fallbackBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            text: i18n("Fallback active")
            visible: overviewPage.hasFansWithState("fallback")
            showCloseButton: true
        }

        // Degraded banner
        Kirigami.InlineMessage {
            id: degradedBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            text: i18n("Fan control degraded")
            visible: !fallbackBanner.visible && overviewPage.hasFansWithState("degraded")
            showCloseButton: true
        }

        // Daemon disconnected banner
        Kirigami.InlineMessage {
            id: disconnectedBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            text: i18n("Couldn't talk to the fan-control daemon. Check that the system service is running, then retry.")
            visible: !statusMonitor.daemonConnected && !fallbackBanner.visible && !degradedBanner.visible
            showCloseButton: true
        }

        // -- Fan list --

        Kirigami.CardsListView {
            id: fanList
            Layout.fillWidth: true
            Layout.fillHeight: true
            model: fanListModel
            implicitHeight: contentHeight

            delegate: FanRowDelegate {
                width: fanList.width
                fanId: model.fanId
                displayName: model.displayName
                supportState: model.supportState
                controlMode: model.controlMode
                fanState: model.state
                temperatureMillidegrees: model.temperatureMillidegrees
                rpm: model.rpm
                outputPercent: model.outputPercent
                hasTach: model.hasTach
                supportReason: model.supportReason
                highTempAlert: model.highTempAlert
                severityOrder: model.severityOrder
                onClicked: {
                    // Fan detail page navigation — Plan 02
                    // pageStack.push(Qt.resolvedUrl("FanDetailPage.qml"), { "fanId": fanId })
                }
            }

            // Empty state
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.largeSpacing * 4
                visible: fanListModel.rowCount() === 0 && statusMonitor.daemonConnected
                icon.name: "dialog-information-symbolic"
                text: i18n("No managed fans yet")
                explanation: i18n("Select a supported fan, choose its temperature source, then validate and apply the draft to start daemon-managed control.")
            }
        }
    }

    // Toolbar action: Wizard configuration (secondary, Plan 04)
    actions.main: Kirigami.Action {
        text: i18n("Wizard configuration")
        iconName: "tools-wizard"
        enabled: statusMonitor.daemonConnected
    }
}