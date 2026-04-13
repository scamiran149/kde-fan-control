/*
 * KDE Fan Control — Overview Fan Row Delegate
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * High-performance overview row bound directly to an OverviewFanRow
 * object property for surgical per-property QML updates.
 *
 * Layout is locked: fixed widths for rapidly changing numeric fields,
 * monospace font, no auto-sizing for hot content.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Kirigami.AbstractCard {
    id: fanRow

    property var rowObject: null

    signal renameRequested(string fanId, string friendlyName, string hardwareLabel)

    Layout.minimumHeight: 56
    Layout.preferredHeight: 56

    contentItem: RowLayout {
        spacing: Kirigami.Units.mediumSpacing

        StateBadge {
            fanState: fanRow.rowObject ? fanRow.rowObject.visualState : "unmanaged"
            stateTextOverride: fanRow.rowObject ? fanRow.rowObject.stateText : ""
            stateIconOverride: fanRow.rowObject ? fanRow.rowObject.stateIconName : ""
            stateColorOverride: fanRow.rowObject ? fanRow.rowObject.stateColor : ""
            highTempAlert: fanRow.rowObject ? fanRow.rowObject.highTempAlert : false
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.minimumWidth: 120
            spacing: 0

            Controls.Label {
                text: fanRow.rowObject ? fanRow.rowObject.displayName : ""
                font.weight: Font.DemiBold
                Layout.fillWidth: true
                elide: Text.ElideRight
                maximumLineCount: 1
            }

            Controls.Label {
                text: fanRow.rowObject && fanRow.rowObject.showSupportReason ? fanRow.rowObject.supportReason : ""
                visible: fanRow.rowObject ? fanRow.rowObject.showSupportReason : false
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
                Layout.fillWidth: true
                elide: Text.ElideRight
                maximumLineCount: 1
            }
        }

        TemperatureDisplay {
            millidegrees: fanRow.rowObject ? fanRow.rowObject.temperatureMillidegrees : 0
            showUnit: true
            showNoSource: fanRow.rowObject ?
                          (fanRow.rowObject.visualState === "unmanaged" ||
                           fanRow.rowObject.visualState === "unavailable" ||
                           fanRow.rowObject.visualState === "partial") : true
            fixedWidth: true
        }

        Controls.Label {
            text: fanRow.rowObject ? fanRow.rowObject.rpmText : ""
            visible: fanRow.rowObject ? fanRow.rowObject.showRpm : false
            font.family: "monospace"
            font.pixelSize: Kirigami.Theme.smallFont.pixelSize
            color: Kirigami.Theme.disabledTextColor
            Layout.minimumWidth: 64
            Layout.preferredWidth: 64
            horizontalAlignment: Text.AlignRight
        }

        OutputBar {
            percent: fanRow.rowObject ? fanRow.rowObject.outputPercent : 0.0
            active: fanRow.rowObject ?
                    (fanRow.rowObject.visualState === "managed" ||
                     fanRow.rowObject.visualState === "degraded" ||
                     fanRow.rowObject.visualState === "fallback") : false
            outputTextOverride: fanRow.rowObject ? fanRow.rowObject.outputText : ""
            fixedWidth: true
        }

        Kirigami.Icon {
            source: "go-next-symbolic"
            Layout.preferredWidth: Kirigami.Units.iconSizes.small
            Layout.preferredHeight: Kirigami.Units.iconSizes.small
            color: Kirigami.Theme.textColor
            Accessible.name: i18n("Open fan details")
        }
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: function(mouse) {
            if (mouse.button === Qt.RightButton) {
                fanRow.renameRequested(
                    fanRow.rowObject ? fanRow.rowObject.fanId : "",
                    fanRow.rowObject ? fanRow.rowObject.friendlyName : "",
                    fanRow.rowObject ? fanRow.rowObject.hardwareLabel : ""
                )
            } else {
                fanRow.clicked()
            }
        }
    }
}