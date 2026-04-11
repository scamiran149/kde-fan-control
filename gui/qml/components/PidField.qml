/*
 * KDE Fan Control — PID Gain Input Field
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Reusable QML component for PID gain input with hover help text.
 * Displays a label, a SpinBox for numeric entry, and a ToolTip
 * with a brief explanation of what the gain controls.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

RowLayout {
    id: pidField

    property string label: "Kp"
    property double value: 1.0
    property string helpText: ""
    property double fromValue: 0.0
    property double toValue: 100.0
    property double stepSize: 0.1
    property int decimals: 2

    signal valueModified(double newValue)

    spacing: Kirigami.Units.smallSpacing

    Kirigami.FormData.label: label

    Controls.SpinBox {
        id: spinBox
        from: Math.round(pidField.fromValue * Math.pow(10, pidField.decimals))
        to: Math.round(pidField.toValue * Math.pow(10, pidField.decimals))
        stepSize: Math.round(pidField.stepSize * Math.pow(10, pidField.decimals))
        value: Math.round(pidField.value * Math.pow(10, pidField.decimals))

        property int decimals: pidField.decimals
        property double realValue: value / Math.pow(10, pidField.decimals)

        textFromValue: function(v) {
            return (v / Math.pow(10, pidField.decimals)).toFixed(pidField.decimals)
        }

        valueFromText: function(text) {
            return Math.round(parseFloat(text) * Math.pow(10, pidField.decimals))
        }

        editable: true

        onValueModified: {
            pidField.valueModified(realValue)
        }

        Controls.ToolTip.visible: hovered
        Controls.ToolTip.text: pidField.helpText
        Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
    }
}