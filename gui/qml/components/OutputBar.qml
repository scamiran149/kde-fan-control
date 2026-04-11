/*
 * KDE Fan Control — Output Bar Component
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * PWM/output percentage bar. Width 96px minimum, height 8px.
 * Shows filled percentage for active fans, disabled appearance otherwise.
 */

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Item {
    id: outputBar
    width: Math.max(96, barLabel.width + barTrack.width + Kirigami.Units.smallSpacing)
    height: 8

    property double percent: 0.0
    property bool active: false

    Rectangle {
        id: barTrack
        anchors.verticalCenter: parent.verticalCenter
        width: 80
        height: 8
        radius: 4
        color: Kirigami.Theme.backgroundColor
        border.color: Kirigami.Theme.disabledTextColor
        border.width: 1
        opacity: outputBar.active ? 1.0 : 0.5

        Rectangle {
            id: barFill
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: Math.min(parent.width, Math.max(0, parent.width * Math.max(0, outputBar.percent) / 100.0))
            radius: 4
            color: outputBar.active ? Kirigami.Theme.highlightColor : Kirigami.Theme.disabledTextColor
        }
    }

    Controls.Label {
        id: barLabel
        anchors.left: barTrack.right
        anchors.leftMargin: Kirigami.Units.smallSpacing
        anchors.verticalCenter: barTrack.verticalCenter
        text: outputBar.active ?
              Math.round(outputBar.percent) + "%" :
              i18n("No control")
        font: Kirigami.Theme.smallFont
        color: outputBar.active ? Kirigami.Theme.textColor : Kirigami.Theme.disabledTextColor
    }
}