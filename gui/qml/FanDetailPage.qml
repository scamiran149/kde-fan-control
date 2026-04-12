/*
 * KDE Fan Control — Fan Detail Page
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Per-fan detail page with core controls, draft editing,
 * auto-tune flow, and advanced tabs.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Kirigami.ScrollablePage {
    id: fanDetailPage

    // --- Required properties set when pushing this page ---
    property string fanId: ""
    property string fanDisplayName: ""
    property string fanSupportState: "unavailable"
    property string fanControlMode: "pwm"
    property string fanState: "unmanaged"
    property int fanTemperatureMillidegrees: 0
    property int fanRpm: 0
    property double fanOutputPercent: -1.0
    property bool fanHasTach: false
    property string fanSupportReason: ""
    property bool fanHighTempAlert: false

    title: fanDisplayName

    // Wizard configuration entry point for unmanaged available fans (Plan 04 per D-04)
    actions.main: Kirigami.Action {
        text: i18n("Wizard configuration")
        iconName: "tools-wizard"
        visible: fanDetailPage.fanSupportState === "available" && fanDetailPage.fanState === "unmanaged"
        enabled: statusMonitor.daemonConnected
        onTriggered: {
            wizardDialog.preselectedFanId = fanDetailPage.fanId
            wizardDialog.open()
        }
    }

    // Load fan data into draft model when page becomes active
    onFanIdChanged: {
        if (fanId !== "") {
            draftModel.loadFan(fanId)
        }
    }

    // --- Helper for sensor multi-select ---

    // Available sensor IDs from sensorListModel
    function availableSensorIds() {
        var ids = []
        for (var i = 0; i < sensorListModel.rowCount(); i++) {
            var idx = sensorListModel.index(i, 0)
            ids.push(sensorListModel.data(idx, SensorListModel.SensorIdRole))
        }
        return ids
    }

    function isSensorSelected(sensorId) {
        return draftModel.sensorIds.indexOf(sensorId) >= 0
    }

    function toggleSensor(sensorId) {
        var current = draftModel.sensorIds.slice()
        var idx = current.indexOf(sensorId)
        if (idx >= 0) {
            current.splice(idx, 1)
        } else {
            current.push(sensorId)
        }
        draftModel.setSensorIdsViaDBus(current)
    }

    // --- Helper for temperature display ---
    function millidegToCelsius(millideg) {
        return (millideg / 1000.0).toFixed(1)
    }

    // --- Auto-tune failure text ---
    property string autoTuneErrorText: ""

    ColumnLayout {
        id: mainLayout
        spacing: Kirigami.Units.mdSpacing

        // ================================================
        // HEADER BLOCK (always visible)
        // ================================================

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.mdSpacing

            // Fan display name
            Kirigami.Heading {
                text: fanDisplayName
                level: 2
                Layout.fillWidth: true
            }

            // State badge
            StateBadge {
                state: fanDetailPage.fanState
                supportState: fanDetailPage.fanSupportState
                highTempAlert: fanDetailPage.fanHighTempAlert
            }
        }

        // Support summary text
        Controls.Label {
            Layout.fillWidth: true
            text: {
                if (fanDetailPage.fanSupportState === "available") {
                    if (fanDetailPage.fanState === "managed") {
                        return i18n("Managed — daemon-controlled")
                    } else if (fanDetailPage.fanState === "fallback") {
                        return i18n("Fallback — driven to maximum speed")
                    } else if (fanDetailPage.fanState === "degraded") {
                        return i18n("Degraded — control issues detected")
                    }
                    return i18n("Available for management")
                } else if (fanDetailPage.fanSupportState === "partial") {
                    return i18n("Partial support: %1").arg(fanDetailPage.fanSupportReason)
                }
                return i18n("Unsupported: %1").arg(fanDetailPage.fanSupportReason)
            }
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.WordWrap
        }

        // Metrics row: temperature, RPM, output
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.lgSpacing

            TemperatureDisplay {
                millidegrees: fanDetailPage.fanTemperatureMillidegrees
                visible: fanDetailPage.fanState === "managed" || fanDetailPage.fanState === "fallback"
            }

            Controls.Label {
                text: fanDetailPage.fanHasTach && fanDetailPage.fanRpm > 0
                    ? i18n("%1 RPM").arg(fanDetailPage.fanRpm)
                    : i18n("No RPM feedback")
                color: Kirigami.Theme.disabledTextColor
            }

            OutputBar {
                percent: fanDetailPage.fanOutputPercent
                enabled: fanDetailPage.fanState === "managed"
                Layout.preferredWidth: 96
                Layout.preferredHeight: 8
            }
        }

        // High-temp alert pill
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            text: i18n("High temperature alert")
            visible: fanDetailPage.fanHighTempAlert
            showCloseButton: true
        }

        // ================================================
        // AUTO-TUNE COMPLETION BANNER (per D-18/D-19)
        // ================================================

        Kirigami.InlineMessage {
            id: autoTuneProposalBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Positive
            visible: draftModel.autoTuneProposalAvailable
            text: i18n("Auto-tune finished")

            actions: [
                Kirigami.Action {
                    text: i18n("Accept proposed gains")
                    onTriggered: {
                        draftModel.acceptAutoTuneProposal()
                    }
                },
                Kirigami.Action {
                    text: i18n("Dismiss proposal")
                    onTriggered: {
                        draftModel.dismissAutoTuneProposal()
                    }
                }
            ]

            Controls.Label {
                text: i18n("Review the proposed PID gains before applying them.")
                wrapMode: Text.WordWrap
                color: Kirigami.Theme.textColor
            }
        }

        // Auto-tune error banner
        Kirigami.InlineMessage {
            id: autoTuneErrorBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            visible: autoTuneErrorText !== ""
            text: autoTuneErrorText
            showCloseButton: true
            onClosed: autoTuneErrorText = ""
        }

        // ================================================
        // VALIDATION / APPLY RESULT BANNERS
        // ================================================

        Kirigami.InlineMessage {
            id: validationSuccessBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Positive
            visible: draftModel.hasValidationError === false && validationAttempted
            text: i18n("Valid: configuration checks passed")
            showCloseButton: true
        }

        property bool validationAttempted: false

        Kirigami.InlineMessage {
            id: validationErrorBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            visible: draftModel.hasValidationError
            text: i18n("%1 rejected entr%2").arg(draftModel.validationErrors.length)
                .arg(draftModel.validationErrors.length === 1 ? "y" : "ies")

            ColumnLayout {
                anchors.left: parent.left
                anchors.right: parent.right
                Repeater {
                    model: draftModel.validationErrors
                    Controls.Label {
                        text: modelData
                        color: Kirigami.Theme.negativeTextColor
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }

        Kirigami.InlineMessage {
            id: applyResultBanner
            Layout.fillWidth: true
            type: draftModel.hasApplyError ? Kirigami.MessageType.Warning : Kirigami.MessageType.Positive
            visible: draftModel.hasApplyError || applySucceeded
            text: draftModel.hasApplyError
                ? i18n("Some fans were rejected during apply")
                : i18n("Changes applied successfully")

            ColumnLayout {
                anchors.left: parent.left
                anchors.right: parent.right
                visible: draftModel.hasApplyError
                Repeater {
                    model: draftModel.applyErrors
                    Controls.Label {
                        text: modelData
                        color: Kirigami.Theme.negativeTextColor
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }

        property bool applySucceeded: false

        // ================================================
        // CORE CONTROLS (per D-13/D-14)
        // ================================================

        Kirigami.FormLayout {
            id: coreControls
            Layout.fillWidth: true
            Layout.maximumWidth: 720

            // 1. Enrollment toggle
            Controls.CheckBox {
                Kirigami.FormData.label: i18n("Managed")
                checked: draftModel.enrolled
                onToggled: draftModel.setEnrolledViaDBus(checked)
                enabled: fanDetailPage.fanSupportState === "available"
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: i18n("Enable daemon-managed control for this fan")
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }

            // 2. Control mode selector
            Controls.ComboBox {
                Kirigami.FormData.label: i18n("Control mode")
                model: {
                    // Show available control modes; currently PWM is primary
                    if (fanDetailPage.fanControlMode === "voltage") {
                        return ["pwm", "voltage"]
                    }
                    return ["pwm"]
                }
                currentIndex: {
                    var mode = draftModel.controlMode
                    if (mode === "voltage") return 1
                    return 0
                }
                onActivated: {
                    var modeText = model[currentIndex]
                    draftModel.setControlModeViaDBus(modeText.toLowerCase())
                }
                enabled: draftModel.enrolled
            }

            // 3. Temperature source selector (multi-select)
            ColumnLayout {
                Kirigami.FormData.label: i18n("Temperature sources")
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                Repeater {
                    model: sensorListModel

                    Controls.CheckBox {
                        text: model.displayName + " (" + millidegToCelsius(model.temperatureMillidegrees) + " °C)"
                        checked: isSensorSelected(model.sensorId)
                        onToggled: toggleSensor(model.sensorId)
                        enabled: draftModel.enrolled
                    }
                }
            }

            // 4. Aggregation selector
            Controls.ComboBox {
                Kirigami.FormData.label: i18n("Aggregation")
                visible: draftModel.sensorIds.length >= 2
                model: ["average", "max", "min", "median"]
                currentIndex: {
                    var agg = draftModel.aggregation
                    if (agg === "max") return 1
                    if (agg === "min") return 2
                    if (agg === "median") return 3
                    return 0
                }
                onActivated: draftModel.setAggregationViaDBus(model[currentIndex])
                enabled: draftModel.enrolled && draftModel.sensorIds.length >= 2
            }

            // Single sensor indicator (shown when exactly 1 sensor selected)
            Controls.Label {
                Kirigami.FormData.label: i18n("Aggregation")
                text: i18n("Single sensor")
                visible: draftModel.sensorIds.length === 1
                color: Kirigami.Theme.disabledTextColor
                enabled: draftModel.enrolled
            }

            // 5. Target temperature field
            Controls.SpinBox {
                Kirigami.FormData.label: i18n("Target temperature")
                from: 0
                to: 1500  // 0.0 to 150.0 °C, stored as tenths
                stepSize: 1
                value: Math.round(draftModel.targetTempCelsius * 10)

                property int decimals: 1
                textFromValue: function(v) { return (v / 10.0).toFixed(1) + " °C" }
                valueFromText: function(text) {
                    var num = parseFloat(text)
                    return Math.round(num * 10)
                }

                onValueModified: {
                    draftModel.setTargetTempCelsiusViaDBus(value / 10.0)
                }
                enabled: draftModel.enrolled
            }

            // 6. PID gains (using PidField component)
            PidField {
                Kirigami.FormData.label: i18n("Kp")
                value: draftModel.kp
                helpText: i18n("Responds to the current temperature gap. Higher values react faster but can hunt.")
                onValueModified: function(newValue) {
                    draftModel.setPidGains(newValue, draftModel.ki, draftModel.kd)
                }
                enabled: draftModel.enrolled
            }

            PidField {
                Kirigami.FormData.label: i18n("Ki")
                value: draftModel.ki
                helpText: i18n("Corrects steady offset over time. Higher values remove drift but can overshoot.")
                onValueModified: function(newValue) {
                    draftModel.setPidGains(draftModel.kp, newValue, draftModel.kd)
                }
                enabled: draftModel.enrolled
            }

            PidField {
                Kirigami.FormData.label: i18n("Kd")
                value: draftModel.kd
                helpText: i18n("Damps fast temperature swings. Higher values can reduce overshoot but may amplify noise.")
                onValueModified: function(newValue) {
                    draftModel.setPidGains(draftModel.kp, draftModel.ki, newValue)
                }
                enabled: draftModel.enrolled
            }

            // 7. Start auto-tune action (per D-17)
            Controls.Button {
                Kirigami.FormData.label: i18n("Auto-tune")
                text: i18n("Start auto-tune")
                icon.name: "run-build-symbolic"
                enabled: draftModel.enrolled && !draftModel.autoTuneRunning
                onClicked: {
                    draftModel.startAutoTune()
                }
                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: i18n("Start automatic PID tuning for this fan")
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }

        // ================================================
        // DRAFT EDITING ACTIONS (per D-03/D-21)
        // ================================================

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.mdSpacing
            spacing: Kirigami.Units.mdSpacing

            Controls.Button {
                text: i18n("Validate draft")
                icon.name: "checkmark-symbolic"
                onClicked: {
                    fanDetailPage.validationAttempted = true
                    fanDetailPage.applySucceeded = false
                    draftModel.validateDraft()
                }
                enabled: statusMonitor.daemonConnected
            }

            Controls.Button {
                text: i18n("Apply changes")
                icon.name: "dialog-ok-apply-symbolic"
                Kirigami.Theme.colorSet: Kirigami.Theme.Button
                highlighted: true
                onClicked: {
                    fanDetailPage.validationAttempted = false
                    fanDetailPage.applySucceeded = false
                    draftModel.applyDraft()
                }
                enabled: statusMonitor.daemonConnected
            }

            Controls.Button {
                text: i18n("Discard draft")
                icon.name: "edit-undo-symbolic"
                onClicked: discardDialog.open()
                enabled: statusMonitor.daemonConnected
            }
        }

        // Discard confirmation dialog (per UI-SPEC)
        Controls.Dialog {
            id: discardDialog
            title: i18n("Discard draft changes")
            modal: true
            standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel

            Controls.Label {
                text: i18n("Discard all staged changes? This keeps the current applied configuration and removes the current draft.")
                wrapMode: Text.WordWrap
            }

            onAccepted: {
                draftModel.discardDraft()
            }
        }

        // ================================================
        // ADVANCED TABS (per D-16)
        // ================================================

        Controls.TabBar {
            id: advancedTabBar
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.xlSpacing

            Controls.TabButton {
                text: i18n("Runtime")
            }
            Controls.TabButton {
                text: i18n("Advanced")
            }
            Controls.TabButton {
                text: i18n("Events")
            }
        }

        Controls.StackLayout {
            Layout.fillWidth: true
            currentIndex: advancedTabBar.currentIndex

            // --- Runtime Tab ---
            ColumnLayout {
                spacing: Kirigami.Units.mdSpacing

                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 720

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Sensors")
                        text: draftModel.sensorIds.join(", ") || i18n("None")
                        wrapMode: Text.WordWrap
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Aggregation")
                        text: draftModel.aggregation || i18n("N/A")
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Target temperature")
                        text: draftModel.targetTempCelsius > 0
                            ? i18n("%1 °C").arg(draftModel.targetTempCelsius.toFixed(1))
                            : i18n("Not set")
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Current error")
                        text: {
                            if (fanDetailPage.fanTemperatureMillidegrees > 0 && draftModel.targetTempMillidegrees > 0) {
                                var error = (fanDetailPage.fanTemperatureMillidegrees - draftModel.targetTempMillidegrees) / 1000.0
                                return i18n("%1 °C").arg(error.toFixed(1))
                            }
                            return i18n("N/A")
                        }
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Output")
                        text: fanDetailPage.fanOutputPercent >= 0
                            ? i18n("%1%").arg(Math.round(fanDetailPage.fanOutputPercent))
                            : i18n("N/A")
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Auto-tune state")
                        text: {
                            if (draftModel.autoTuneRunning) return i18n("Running")
                            if (draftModel.autoTuneProposalAvailable) return i18n("Completed")
                            return i18n("Idle")
                        }
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("High-temp alert")
                        text: fanDetailPage.fanHighTempAlert ? i18n("Active") : i18n("None")
                        color: fanDetailPage.fanHighTempAlert ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
                    }
                }
            }

            // --- Advanced Tab ---
            ColumnLayout {
                spacing: Kirigami.Units.mdSpacing

                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 720

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Sample interval (ms)")
                        from: 250
                        to: 60000
                        stepSize: 100
                        value: 1000
                        enabled: draftModel.enrolled
                        // These advanced fields will be wired via draft model
                        // in a future enhancement once the daemon exposes them
                        // in setDraftFanControlProfile
                    }

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Control interval (ms)")
                        from: 250
                        to: 60000
                        stepSize: 100
                        value: 2000
                        enabled: draftModel.enrolled
                    }

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Write interval (ms)")
                        from: 250
                        to: 60000
                        stepSize: 100
                        value: 2000
                        enabled: draftModel.enrolled
                    }

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Deadband (°C)")
                        from: 0
                        to: 1000  // 0.0 to 100.0 °C as tenths
                        stepSize: 1
                        value: 10 // 1.0 °C default
                        textFromValue: function(v) { return (v / 10.0).toFixed(1) + " °C" }
                        valueFromText: function(text) { return Math.round(parseFloat(text) * 10) }
                        enabled: draftModel.enrolled
                    }

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Minimum output (%)")
                        from: 0
                        to: 100
                        stepSize: 1
                        value: 0
                        textFromValue: function(v) { return v + "%" }
                        valueFromText: function(text) { return parseInt(text) || 0 }
                        enabled: draftModel.enrolled
                    }

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Maximum output (%)")
                        from: 0
                        to: 100
                        stepSize: 1
                        value: 100
                        textFromValue: function(v) { return v + "%" }
                        valueFromText: function(text) { return parseInt(text) || 0 }
                        enabled: draftModel.enrolled
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("PID limits")
                        text: i18n("Integral and derivative clamps are shown in the daemon-provided defaults.")
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.WordWrap
                    }
                }
            }

            // --- Events Tab ---
            ColumnLayout {
                spacing: Kirigami.Units.mdSpacing

                Controls.Label {
                    text: i18n("Lifecycle Events")
                    font.bold: true
                }

                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: 200
                    model: lifecycleEventModel
                    clip: true

                    delegate: Kirigami.AbstractCard {
                        width: ListView.view.width
                        contentItem: RowLayout {
                            spacing: Kirigami.Units.mdSpacing

                            Controls.Label {
                                text: model.timestamp
                                color: Kirigami.Theme.disabledTextColor
                                font.pointSize: Kirigami.Theme.smallFont.pointSize
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0

                                Controls.Label {
                                    text: model.reason
                                    font.bold: true
                                    wrapMode: Text.WordWrap
                                }

                                Controls.Label {
                                    text: model.detail
                                    visible: model.detail !== ""
                                    color: Kirigami.Theme.disabledTextColor
                                    wrapMode: Text.WordWrap
                                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                                }

                                Controls.Label {
                                    text: i18n("Fan: %1").arg(model.fanId)
                                    visible: model.fanId !== ""
                                    color: Kirigami.Theme.disabledTextColor
                                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                                }
                            }

                            Kirigami.Icon {
                                source: {
                                    var type = model.eventType
                                    if (type === "fallback_active") return "dialog-error-symbolic"
                                    if (type === "fan_missing" || type === "temp_source_missing") return "data-warning-symbolic"
                                    return "dialog-information-symbolic"
                                }
                                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                Layout.preferredHeight: Kirigami.Units.iconSizes.small
                            }
                        }
                    }

                    Kirigami.PlaceholderMessage {
                        anchors.centerIn: parent
                        width: parent.width - Kirigami.Units.largeSpacing * 4
                        visible: lifecycleEventModel.rowCount() === 0
                        text: i18n("No lifecycle events recorded")
                    }
                }
            }
        }
    }

    // Refresh lifecycle events when page activates
    Connections {
        target: daemonInterface
        function onLifecycleEventsResult(json) {
            lifecycleEventModel.refresh(json)
        }
    }

    Component.onCompleted: {
        if (fanId !== "") {
            daemonInterface.lifecycleEvents()
        }
    }
}