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

Kirigami.Page {
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
    property string fanFriendlyName: ""
    property string fanLabel: ""
    property bool validationAttempted: false
    property bool applySucceeded: false

    title: fanDisplayName

    actions: [
        Kirigami.Action {
            text: i18n("Back")
            icon.name: "go-previous"
            onTriggered: pageStack.pop()
        },
        Kirigami.Action {
            text: i18n("Rename")
            icon.name: "edit-rename"
            enabled: statusMonitor.daemonConnected && daemonInterface.canWrite
            onTriggered: {
                fanDetailRenameDialog.openFor("fan", fanDetailPage.fanId, fanDetailPage.fanFriendlyName, fanDetailPage.fanLabel)
            }
        },
        Kirigami.Action {
            text: i18n("Wizard configuration")
            icon.name: "tools-wizard"
            visible: fanDetailPage.fanSupportState === "available" && fanDetailPage.fanState === "unmanaged"
            enabled: statusMonitor.daemonConnected && daemonInterface.canWrite
            onTriggered: {
                wizardDialog.preselectedFanId = fanDetailPage.fanId
                wizardDialog.open()
            }
        }
    ]

    // Load fan data into draft model when page becomes active.
    // Defer loading to avoid "Created graphical object was not placed
    // in the graphics scene" warnings from Kirigami.ScrollablePage.
    onFanIdChanged: {
        if (fanId !== "") {
            Qt.callLater(function() {
                refreshFanSnapshotFromModel()
                draftModel.loadFan(fanId)
            })
        }
    }

    function refreshFanSnapshotFromModel() {
        if (fanId === "") {
            return
        }

        var snapshot = fanListModel.fanById(fanId)
        if (!snapshot || snapshot.fanId === undefined) {
            return
        }

        fanDisplayName = snapshot.displayName
        fanFriendlyName = snapshot.friendlyName
        fanLabel = snapshot.label
        fanSupportState = snapshot.supportState
        fanControlMode = snapshot.controlMode
        fanState = snapshot.state
        fanTemperatureMillidegrees = snapshot.temperatureMillidegrees
        fanRpm = snapshot.rpm
        fanOutputPercent = snapshot.outputPercent
        fanHasTach = snapshot.hasTach
        fanSupportReason = snapshot.supportReason
        fanHighTempAlert = snapshot.highTempAlert
    }

    function sensorSummaryText(sensorIds) {
        var names = []
        for (var s = 0; s < sensorIds.length; s++) {
            var sensorId = sensorIds[s]
            for (var i = 0; i < sensorListModel.rowCount(); i++) {
                var idx = sensorListModel.index(i, 0)
                if (sensorListModel.data(idx, SensorListModel.SensorIdRole) === sensorId) {
                    names.push(sensorListModel.data(idx, SensorListModel.DisplayNameRole))
                    break
                }
            }
        }
        return names.join(", ")
    }

    // --- Auto-tune failure text ---
    property string autoTuneErrorText: ""

    Controls.ScrollView {
        id: scrollView
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            id: mainLayout
            spacing: Kirigami.Units.mediumSpacing
            width: scrollView.availableWidth

        // ================================================
        // HEADER BLOCK (always visible)
        // ================================================

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.mediumSpacing

            // Fan display name
            Kirigami.Heading {
                text: fanDisplayName
                level: 2
                Layout.fillWidth: true
            }

            // State badge
            StateBadge {
                fanState: fanDetailPage.fanState
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
                    return i18n("Partial support: %1", fanDetailPage.fanSupportReason)
                }
                return i18n("Unsupported: %1", fanDetailPage.fanSupportReason)
            }
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.WordWrap
        }

        // Metrics row: temperature, RPM, output
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            TemperatureDisplay {
                millidegrees: fanDetailPage.fanTemperatureMillidegrees
                visible: fanDetailPage.fanState === "managed" || fanDetailPage.fanState === "fallback"
            }

            Controls.Label {
                text: fanDetailPage.fanHasTach && fanDetailPage.fanRpm > 0
                    ? i18n("%1 RPM", fanDetailPage.fanRpm)
                    : i18n("No RPM feedback")
                color: Kirigami.Theme.disabledTextColor
            }

            OutputBar {
                percent: fanDetailPage.fanOutputPercent
                active: fanDetailPage.fanState === "managed" ||
                        fanDetailPage.fanState === "degraded" ||
                        fanDetailPage.fanState === "fallback"
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

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Information
            text: i18n("This session is read-only. Click Unlock in the menu to authorize changes.")
            visible: statusMonitor.daemonConnected && !daemonInterface.canWrite
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
            text: i18n(
                "Auto-tune finished. Proposed PID gains: Kp %1, Ki %2, Kd %3.",
                draftModel.proposedKp.toFixed(2),
                draftModel.proposedKi.toFixed(2),
                draftModel.proposedKd.toFixed(2)
            )

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

        }

        // Auto-tune error banner
        Kirigami.InlineMessage {
            id: autoTuneErrorBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            visible: autoTuneErrorText !== ""
            text: autoTuneErrorText
            showCloseButton: true
        }

        // ================================================
        // VALIDATION / APPLY RESULT BANNERS
        // ================================================

        Kirigami.InlineMessage {
            id: validationSuccessBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Positive
            visible: draftModel.hasValidationError === false && fanDetailPage.validationAttempted
            text: i18n("Valid: configuration checks passed")
            showCloseButton: true
        }

        Kirigami.InlineMessage {
            id: validationErrorBanner
            Layout.fillWidth: true
            type: Kirigami.MessageType.Error
            visible: draftModel.hasValidationError
            text: i18np("%1 rejected entry", "%1 rejected entries", draftModel.validationErrors.length)
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            visible: draftModel.hasValidationError

            Repeater {
                model: draftModel.validationErrors
                Controls.Label {
                    Layout.fillWidth: true
                    text: modelData
                    color: Kirigami.Theme.negativeTextColor
                    wrapMode: Text.WordWrap
                }
            }
        }

        Kirigami.InlineMessage {
            id: applyResultBanner
            Layout.fillWidth: true
            type: draftModel.hasApplyError ? Kirigami.MessageType.Warning : Kirigami.MessageType.Positive
            visible: draftModel.hasApplyError || fanDetailPage.applySucceeded
            text: draftModel.hasApplyError
                ? i18n("Some fans were rejected during apply")
                : i18n("Changes applied successfully")
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            visible: draftModel.hasApplyError

            Repeater {
                model: draftModel.applyErrors
                Controls.Label {
                    Layout.fillWidth: true
                    text: modelData
                    color: Kirigami.Theme.negativeTextColor
                    wrapMode: Text.WordWrap
                }
            }
        }

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
                enabled: fanDetailPage.fanSupportState === "available" && daemonInterface.canWrite
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
                enabled: draftModel.enrolled && daemonInterface.canWrite
            }

            // 3. Temperature source selector (multi-select dropdown)
            MultiSelectComboBox {
                Kirigami.FormData.label: i18n("Temperature sources")
                Layout.fillWidth: true
                listModel: sensorListModel
                idRole: "sensorId"
                displayRole: "displayName"
                detailRole: "temperatureMillidegrees"
                detailFormatter: function(millideg) { return (millideg / 1000.0).toFixed(1) + " °C" }
                selectedIds: draftModel.sensorIds
                summaryText: fanDetailPage.sensorSummaryText(draftModel.sensorIds)
                enabled: draftModel.enrolled && daemonInterface.canWrite
                placeholderText: i18n("Select sensors…")
                onSelectionChanged: function(newIds) {
                    draftModel.setSensorIdsViaDBus(newIds)
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
                enabled: draftModel.enrolled && draftModel.sensorIds.length >= 2 && daemonInterface.canWrite
            }

            // Single sensor indicator (shown when exactly 1 sensor selected)
            Controls.Label {
                Kirigami.FormData.label: i18n("Aggregation")
                text: i18n("Single sensor")
                visible: draftModel.sensorIds.length === 1
                color: Kirigami.Theme.disabledTextColor
                enabled: draftModel.enrolled && daemonInterface.canWrite
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
                enabled: draftModel.enrolled && daemonInterface.canWrite
            }

            // 6. PID gains (using PidField component)
            PidField {
                Kirigami.FormData.label: i18n("Kp")
                value: draftModel.kp
                helpText: i18n("Responds to the current temperature gap. Higher values react faster but can hunt.")
                onValueModified: function(newValue) {
                    draftModel.setPidGains(newValue, draftModel.ki, draftModel.kd)
                }
                enabled: draftModel.enrolled && daemonInterface.canWrite
            }

            PidField {
                Kirigami.FormData.label: i18n("Ki")
                value: draftModel.ki
                helpText: i18n("Corrects steady offset over time. Higher values remove drift but can overshoot.")
                onValueModified: function(newValue) {
                    draftModel.setPidGains(draftModel.kp, newValue, draftModel.kd)
                }
                enabled: draftModel.enrolled && daemonInterface.canWrite
            }

            PidField {
                Kirigami.FormData.label: i18n("Kd")
                value: draftModel.kd
                helpText: i18n("Damps fast temperature swings. Higher values can reduce overshoot but may amplify noise.")
                onValueModified: function(newValue) {
                    draftModel.setPidGains(draftModel.kp, draftModel.ki, newValue)
                }
                enabled: draftModel.enrolled && daemonInterface.canWrite
            }

            // 7. Start auto-tune action (per D-17)
            Controls.Button {
                Kirigami.FormData.label: i18n("Auto-tune")
                text: i18n("Start auto-tune")
                icon.name: "run-build-symbolic"
                enabled: draftModel.enrolled && !draftModel.autoTuneRunning && daemonInterface.canWrite
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
            Layout.topMargin: Kirigami.Units.mediumSpacing
            spacing: Kirigami.Units.mediumSpacing

            Controls.Button {
                text: i18n("Validate draft")
                icon.name: "checkmark-symbolic"
                onClicked: {
                    fanDetailPage.validationAttempted = true
                    fanDetailPage.applySucceeded = false
                    draftModel.validateDraft()
                }
                enabled: statusMonitor.daemonConnected && daemonInterface.canWrite
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
                enabled: statusMonitor.daemonConnected && daemonInterface.canWrite
            }

            Controls.Button {
                text: i18n("Discard draft")
                icon.name: "edit-undo-symbolic"
                onClicked: discardDialog.open()
                enabled: statusMonitor.daemonConnected && daemonInterface.canWrite
            }
        }

        // Discard confirmation dialog (per UI-SPEC)
        Controls.Popup {
            id: discardDialog
            modal: true
            focus: true
            closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside
            anchors.centerIn: parent
            width: Math.min(parent.width * 0.6, 420)
            padding: Kirigami.Units.largeSpacing

            background: Rectangle {
                radius: 8
                color: Kirigami.Theme.backgroundColor
                border.color: Kirigami.Theme.disabledTextColor
            }

            contentItem: ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing

                Kirigami.Heading {
                    text: i18n("Discard draft changes")
                    level: 3
                }

                Controls.Label {
                    Layout.fillWidth: true
                    text: i18n("Discard all staged changes? This keeps the current applied configuration and removes the current draft.")
                    wrapMode: Text.WordWrap
                }

                RowLayout {
                    Layout.fillWidth: true

                    Item { Layout.fillWidth: true }

                    Controls.Button {
                        text: i18n("Cancel")
                        onClicked: discardDialog.close()
                    }

                    Controls.Button {
                        text: i18n("Discard")
                        icon.name: "edit-delete-remove"
                        highlighted: true
                        onClicked: {
                            draftModel.discardDraft()
                            discardDialog.close()
                        }
                    }
                }
            }
        }

        // ================================================
        // ADVANCED TABS (per D-16)
        // ================================================

        Controls.ComboBox {
            id: advancedSectionSelector
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.xlSpacing
            model: [i18n("Runtime"), i18n("Advanced"), i18n("Events")]
        }

        StackLayout {
            Layout.fillWidth: true
            currentIndex: advancedSectionSelector.currentIndex

            // --- Runtime Tab ---
            ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing

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
                            ? i18n("%1 °C", draftModel.targetTempCelsius.toFixed(1))
                            : i18n("Not set")
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Current error")
                        text: {
                            if (fanDetailPage.fanTemperatureMillidegrees > 0 && draftModel.targetTempMillidegrees > 0) {
                                var error = (fanDetailPage.fanTemperatureMillidegrees - draftModel.targetTempMillidegrees) / 1000.0
                                return i18n("%1 °C", error.toFixed(1))
                            }
                            return i18n("N/A")
                        }
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Output")
                        text: fanDetailPage.fanOutputPercent >= 0
                            ? i18n("%1%", Math.round(fanDetailPage.fanOutputPercent))
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
                spacing: Kirigami.Units.mediumSpacing

                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 720

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Sample interval (ms)")
                        from: 250
                        to: 60000
                        stepSize: 100
                        value: draftModel.sampleIntervalMs
                        onValueModified: draftModel.setAdvancedCadence(value, draftModel.controlIntervalMs, draftModel.writeIntervalMs)
                        enabled: draftModel.enrolled
                    }

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Control interval (ms)")
                        from: 250
                        to: 60000
                        stepSize: 100
                        value: draftModel.controlIntervalMs
                        onValueModified: draftModel.setAdvancedCadence(draftModel.sampleIntervalMs, value, draftModel.writeIntervalMs)
                        enabled: draftModel.enrolled
                    }

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Write interval (ms)")
                        from: 250
                        to: 60000
                        stepSize: 100
                        value: draftModel.writeIntervalMs
                        onValueModified: draftModel.setAdvancedCadence(draftModel.sampleIntervalMs, draftModel.controlIntervalMs, value)
                        enabled: draftModel.enrolled
                    }

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Deadband (°C)")
                        from: 0
                        to: 1000  // 0.0 to 100.0 °C as tenths
                        stepSize: 1
                        value: Math.round(draftModel.deadbandMillidegrees / 100)
                        textFromValue: function(v) { return (v / 10.0).toFixed(1) + " °C" }
                        valueFromText: function(text) { return Math.round(parseFloat(text) * 10) }
                        onValueModified: draftModel.setDeadbandMillidegrees(value * 100)
                        enabled: draftModel.enrolled
                    }

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Minimum output (%)")
                        from: 0
                        to: 100
                        stepSize: 1
                        value: Math.round(draftModel.outputMinPercent)
                        textFromValue: function(v) { return v + "%" }
                        valueFromText: function(text) { return parseInt(text) || 0 }
                        onValueModified: draftModel.setOutputRange(value, draftModel.outputMaxPercent)
                        enabled: draftModel.enrolled
                    }

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Maximum output (%)")
                        from: 0
                        to: 100
                        stepSize: 1
                        value: Math.round(draftModel.outputMaxPercent)
                        textFromValue: function(v) { return v + "%" }
                        valueFromText: function(text) { return parseInt(text) || 0 }
                        onValueModified: draftModel.setOutputRange(draftModel.outputMinPercent, value)
                        enabled: draftModel.enrolled
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("PID limits")
                        text: i18n("Integral and derivative clamps are set by daemon defaults.")
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.WordWrap
                    }
                }
            }

            // --- Events Tab ---
            ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing

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
                            spacing: Kirigami.Units.mediumSpacing

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
                                    text: i18n("Fan: %1", model.fanId)
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
    }

    // Refresh lifecycle events when page activates
    Connections {
        target: daemonInterface
        function onSnapshotResult(json) {
            Qt.callLater(fanDetailPage.refreshFanSnapshotFromModel)
        }
        function onRuntimeStateResult(json) {
            Qt.callLater(fanDetailPage.refreshFanSnapshotFromModel)
        }
        function onDraftConfigResult(json) {
            Qt.callLater(fanDetailPage.refreshFanSnapshotFromModel)
        }
        function onLifecycleEventsResult(json) {
            lifecycleEventModel.refresh(json)
        }
    }

    Connections {
        target: fanListModel
        function onModelReset() {
            fanDetailPage.refreshFanSnapshotFromModel()
        }
    }

    Component.onCompleted: {
        if (fanId !== "") {
            Qt.callLater(function() {
                refreshFanSnapshotFromModel()
                daemonInterface.lifecycleEvents()
            })
        }
    }

    RenameDialog {
        id: fanDetailRenameDialog
    }
}
