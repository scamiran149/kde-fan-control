/*
 * KDE Fan Control — Fan Row Delegate
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Compact delegate for each fan entry in the overview list.
 * Shows: display name, state badge, temperature, RPM, output bar, support reason.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Kirigami.AbstractCard {
    id: fanRow

    // Required properties from model
    property string fanId: ""
    property string displayName: ""
    property string supportState: ""
    property string controlMode: ""
    property string fanState: ""
    property int temperatureMillidegrees: 0
    property int rpm: 0
    property double outputPercent: 0.0
    property bool hasTach: false
    property string supportReason: ""
    property bool highTempAlert: false
    property int severityOrder: 0

    Layout.minimumHeight: 56

    contentItem: RowLayout {
        spacing: Kirigami.Units.mediumSpacing

        // State badge
        StateBadge {
            fanState: fanRow.fanState
            highTempAlert: fanRow.highTempAlert
        }

        // Fan name and support reason
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0

            Controls.Label {
                text: fanRow.displayName
                font.weight: Font.DemiBold
                Layout.fillWidth: true
                elide: Text.ElideRight
            }

            Controls.Label {
                text: fanRow.supportReason
                visible: fanRow.supportReason.length > 0 &&
                         (fanRow.fanState === "partial" ||
                          fanRow.fanState === "unavailable" ||
                          fanRow.fanState === "degraded")
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
        }

        // Temperature display
        TemperatureDisplay {
            millidegrees: fanRow.temperatureMillidegrees
            showUnit: true
            showNoSource: fanRow.fanState === "unmanaged" ||
                          fanRow.fanState === "unavailable" ||
                          fanRow.fanState === "partial"
        }

        // RPM
        Controls.Label {
            text: fanRow.hasTach ?
                  (fanRow.rpm > 0 ? fanRow.rpm + " RPM" : "0 RPM") :
                  i18n("No RPM feedback")
            font: Kirigami.Theme.smallFont
            color: Kirigami.Theme.disabledTextColor
            visible: fanRow.fanState === "managed" ||
                     fanRow.fanState === "degraded" ||
                     fanRow.fanState === "fallback"
        }

        // Output bar
        OutputBar {
            percent: fanRow.outputPercent
            active: fanRow.fanState === "managed" ||
                     fanRow.fanState === "degraded" ||
                     fanRow.fanState === "fallback"
        }

        // Chevron to open fan detail
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
        onClicked: fanRow.clicked()
    }
}
