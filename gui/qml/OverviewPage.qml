/*
 * KDE Fan Control — Overview Page
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Shows all fans with severity banners and live metrics.
 * Uses overviewModel (OverviewModel) for the fast telemetry/
 * structural split path, binding delegates directly to
 * OverviewFanRow objects via the rowObject role.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Kirigami.ScrollablePage {
    id: overviewPage
    title: i18n("Fan Control")

    property bool _pageActive: false

    onIsCurrentPageChanged: {
        if (isCurrentPage) {
            _pageActive = true
            statusMonitor.forceStructureRefresh()
        } else {
            _pageActive = false
        }
    }

    function hasFansWithVisualState(stateName) {
        for (var i = 0; i < overviewModel.rowCount(); i++) {
            var idx = overviewModel.index(i, 0)
            var vs = overviewModel.data(idx, OverviewModel.VisualStateRole)
            if (vs === stateName) return true
        }
        return false
    }

    ColumnLayout {
        id: mainLayout
        spacing: Kirigami.Units.mediumSpacing

        // -- Severity banners (use visualState from overview model) --

        Kirigami.InlineMessage {
            id: fallbackBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            text: i18n("Fallback active")
            visible: overviewPage.hasFansWithVisualState("fallback")
            showCloseButton: true
        }

        Kirigami.InlineMessage {
            id: degradedBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            text: i18n("Fan control degraded")
            visible: !fallbackBanner.visible && overviewPage.hasFansWithVisualState("degraded")
            showCloseButton: true
        }

        Kirigami.InlineMessage {
            id: disconnectedBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            text: i18n("Couldn't talk to the fan-control daemon. Check that the system service is running, then retry.")
            visible: !statusMonitor.daemonConnected && !fallbackBanner.visible && !degradedBanner.visible
            showCloseButton: true
        }

        // -- Fan list (overview path) --

        Kirigami.CardsListView {
            id: fanList
            Layout.fillWidth: true
            Layout.fillHeight: true
            model: overviewModel
            implicitHeight: contentHeight

            delegate: FanRowDelegate {
                width: fanList.width
                rowObject: model.rowObject

                onClicked: {
                    var fanIdStr = rowObject ? rowObject.fanId : ""
                    if (fanIdStr.length === 0) return

                    // For now, navigate to detail page using fanListModel data
                    // (detail pages still use the legacy path)
                    var detailIdx = -1
                    for (var i = 0; i < fanListModel.rowCount(); i++) {
                        var idx = fanListModel.index(i, 0)
                        if (fanListModel.data(idx, FanListModel.FanIdRole) === fanIdStr) {
                            detailIdx = i
                            break
                        }
                    }
                    if (detailIdx < 0) return

                    var idx = fanListModel.index(detailIdx, 0)
                    var pageProps = {
                        "fanId": fanIdStr,
                        "fanDisplayName": fanListModel.data(idx, FanListModel.DisplayNameRole),
                        "fanSupportState": fanListModel.data(idx, FanListModel.SupportStateRole),
                        "fanControlMode": fanListModel.data(idx, FanListModel.ControlModeRole),
                        "fanState": fanListModel.data(idx, FanListModel.StateRole),
                        "fanTemperatureMillidegrees": fanListModel.data(idx, FanListModel.TemperatureMillidegRole),
                        "fanRpm": fanListModel.data(idx, FanListModel.RpmRole),
                        "fanOutputPercent": fanListModel.data(idx, FanListModel.OutputPercentRole),
                        "fanHasTach": fanListModel.data(idx, FanListModel.HasTachRole),
                        "fanSupportReason": fanListModel.data(idx, FanListModel.SupportReasonRole),
                        "fanHighTempAlert": fanListModel.data(idx, FanListModel.HighTempAlertRole),
                        "fanFriendlyName": fanListModel.data(idx, FanListModel.FriendlyNameRole),
                        "fanLabel": fanListModel.data(idx, FanListModel.LabelRole)
                    }
                    if (pageStack.currentItem && pageStack.currentItem.toString().indexOf("FanDetailPage") !== -1) {
                        pageStack.replace(Qt.resolvedUrl("FanDetailPage.qml"), pageProps)
                    } else {
                        pageStack.push(Qt.resolvedUrl("FanDetailPage.qml"), pageProps)
                    }
                }
                onRenameRequested: function(fId, fFriendlyName, fLabel) {
                    overviewRenameDialog.openFor("fan", fId, fFriendlyName, fLabel)
                }
            }

            // Empty state
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.largeSpacing * 4
                visible: fanList.count === 0 && statusMonitor.daemonConnected
                icon.name: "dialog-information-symbolic"
                text: i18n("No managed fans yet")
                explanation: i18n("Select a supported fan, choose its temperature source, then validate and apply the draft to start daemon-managed control.")
            }

            // Empty-state wizard CTA (Plan 04)
            Controls.Button {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: Kirigami.Units.mediumSpacing
                visible: fanList.count === 0 && statusMonitor.daemonConnected
                text: i18n("Wizard configuration")
                icon.name: "tools-wizard"
                highlighted: true
                enabled: daemonInterface.canWrite
                onClicked: {
                    wizardDialog.preselectedFanId = ""
                    wizardDialog.open()
                }
            }
        }
    }

    // Toolbar action: Wizard configuration (secondary per D-04)
    actions: Kirigami.Action {
        text: i18n("Wizard configuration")
        icon.name: "tools-wizard"
        enabled: statusMonitor.daemonConnected && daemonInterface.canWrite
        onTriggered: {
            wizardDialog.preselectedFanId = ""
            wizardDialog.open()
        }
    }

    RenameDialog {
        id: overviewRenameDialog
    }
}
