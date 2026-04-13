/*
 * KDE Fan Control — Wizard Configuration Dialog
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Optional guided setup wizard for fan management enrollment.
 * Per UI-SPEC Wizard Contract:
 *   Step 1: Select fan
 *   Step 2: Select control mode
 *   Step 3: Select sensor source(s)
 *   Step 4: Choose aggregation (conditional, only when 2+ sensors)
 *   Step 5: Set target temperature
 *   Step 6: Review starter PID values
 *   Step 7: Review + validate + apply
 *
 * Uses the same draft/apply contract as direct editing (DraftModel).
 * Never bypasses validation. Ends on review screen with explicit apply.
 * No advanced controls (cadence, deadband, actuator policy, PID limits).
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Kirigami.Dialog {
    id: wizardDialog

    title: i18n("Wizard Configuration")

    // Whether a specific fan was pre-selected (e.g. from Fan Detail unmanaged entry)
    property string preselectedFanId: ""

    // Internal state
    property int currentStep: 0
    property string selectedFanId: ""
    property string selectedControlMode: "pwm"
    property var selectedSensorIds: []
    property string selectedAggregation: "average"
    property double selectedTargetTempCelsius: 65.0

    // Track whether we've loaded a fan into the draft model
    property bool draftLoaded: false

    // Validation / apply state
    property bool validationAttempted: false
    property bool applyAttempted: false

    // Total steps (aggregation step is conditional)
    readonly property int totalSteps: 7

    // Whether the aggregation step should be shown
    readonly property bool showAggregationStep: selectedSensorIds.length >= 2

    // Map logical step index to display step number (skipping aggregation if hidden)
    readonly property int displayStepNumber: {
        if (currentStep <= 3) return currentStep + 1  // Steps 0-3 map to 1-4
        if (!showAggregationStep && currentStep >= 4) return currentStep  // Shift down by 1
        return currentStep + 1
    }

    readonly property int totalDisplaySteps: showAggregationStep ? 7 : 6

    // Step indices: 0=fan, 1=controlmode, 2=sensors, 3=aggregation, 4=targettemp, 5=pid, 6=review
    readonly property int stepFan: 0
    readonly property int stepControlMode: 1
    readonly property int stepSensor: 2
    readonly property int stepAggregation: 3
    readonly property int stepTargetTemp: 4
    readonly property int stepPid: 5
    readonly property int stepReview: 6

    modal: true
    width: Math.min(Math.max(parent.width * 0.6, 560), 720)
    height: Math.min(Math.max(parent.height * 0.7, 480), 640)

    standardButtons: Kirigami.Dialog.NoButton

    // Reset wizard when opening
    onOpened: {
        console.log("KFC_GUI_DEBUG Wizard: opened, preselectedFanId=" + preselectedFanId)
        resetWizard()
        if (preselectedFanId !== "") {
            selectedFanId = preselectedFanId
            currentStep = stepControlMode
            loadFanIntoDraft(preselectedFanId)
        }
    }

    function resetWizard() {
        console.log("KFC_GUI_DEBUG Wizard: resetWizard")
        currentStep = 0
        selectedFanId = ""
        selectedControlMode = "pwm"
        selectedSensorIds = []
        selectedAggregation = "average"
        selectedTargetTempCelsius = 65.0
        draftLoaded = false
        validationAttempted = false
        applyAttempted = false
    }

    function loadFanIntoDraft(fanId) {
        console.log("KFC_GUI_DEBUG Wizard: loadFanIntoDraft fanId=" + fanId)
        selectedFanId = fanId
        draftModel.loadFan(fanId)
        draftModel.setEnrolledViaDBus(true)
        draftLoaded = true
    }

    // Helper to get available control modes for the selected fan
    function availableControlModes() {
        // Default to PWM; add voltage if fan supports it
        if (selectedFanId === "") return ["pwm"]
        for (var i = 0; i < fanListModel.rowCount(); i++) {
            var idx = fanListModel.index(i, 0)
            var id = fanListModel.data(idx, FanListModel.FanIdRole)
            if (id === selectedFanId) {
                var mode = fanListModel.data(idx, FanListModel.ControlModeRole)
                if (mode === "voltage") return ["pwm", "voltage"]
                return ["pwm"]
            }
        }
        return ["pwm"]
    }

    // Helper to check if a fan is eligible for wizard enrollment
    function isFanEligible(fanId) {
        for (var i = 0; i < fanListModel.rowCount(); i++) {
            var idx = fanListModel.index(i, 0)
            var id = fanListModel.data(idx, FanListModel.FanIdRole)
            var supportState = fanListModel.data(idx, FanListModel.SupportStateRole)
            var state = fanListModel.data(idx, FanListModel.StateRole)
            if (id === fanId) {
                return supportState === "available" && state === "unmanaged"
            }
        }
        return false
    }

    // Navigate to next step (skip aggregation if not needed)
    function goNext() {
        console.log("KFC_GUI_DEBUG Wizard: goNext from step " + currentStep)
        if (currentStep === stepSensor) {
            // Coming from sensor selection — skip aggregation if only 1 sensor
            if (selectedSensorIds.length < 2) {
                currentStep = stepTargetTemp
            } else {
                currentStep = stepAggregation
            }
        } else if (currentStep < stepReview) {
            currentStep++
        }
    }

    // Navigate to previous step (skip aggregation if not needed)
    function goBack() {
        console.log("KFC_GUI_DEBUG Wizard: goBack from step " + currentStep)
        if (currentStep === stepTargetTemp && selectedSensorIds.length < 2) {
            // Skip aggregation step going back
            currentStep = stepSensor
        } else if (currentStep > 0) {
            currentStep--
        }
    }

    // Discard draft and close on cancellation
    function cancelWizard() {
        console.log("KFC_GUI_DEBUG Wizard: cancelWizard draftLoaded=" + draftLoaded)
        if (draftLoaded) {
            draftModel.discardDraft()
        }
        wizardDialog.close()
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.mediumSpacing

        // Progress indicator
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                text: i18n("Step %1 of %2", displayStepNumber, totalDisplaySteps)
                font.bold: true
            }

            Controls.ProgressBar {
                Layout.fillWidth: true
                from: 0
                to: totalDisplaySteps
                value: displayStepNumber
            }
        }

        // Step content
        StackLayout {
            id: stepStack
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: currentStep

            // ============================================
            // STEP 0: Select Fan
            // ============================================
            ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing

                Kirigami.Heading {
                    text: i18n("Select a Fan")
                    level: 3
                }

                Controls.Label {
                    text: i18n("Choose an available, unmanaged fan to set up for daemon-managed control.")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: Kirigami.Theme.disabledTextColor
                }

                ListView {
                    id: fanSelectionList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: 120
                    clip: true
                    model: fanListModel

                    delegate: Kirigami.AbstractCard {
                        width: fanSelectionList.width
                        visible: model.supportState === "available" && model.state === "unmanaged"
                        height: visible ? implicitHeight : 0

                        contentItem: RowLayout {
                            spacing: Kirigami.Units.mediumSpacing

                            Kirigami.Icon {
                                source: "fan-symbolic"
                                Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                                Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0

                                Controls.Label {
                                    text: model.displayName
                                    font.weight: Font.DemiBold
                                }
                                Controls.Label {
                                    text: model.controlMode ? i18n("Control: %1", model.controlMode) : ""
                                    font: Kirigami.Theme.smallFont
                                    color: Kirigami.Theme.disabledTextColor
                                }
                            }

                            Controls.RadioButton {
                                checked: selectedFanId === model.fanId
                                onToggled: {
                                    selectedFanId = model.fanId
                                }
                            }
                        }
                    }
                }

                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    type: Kirigami.MessageType.Information
                    visible: selectedFanId === ""
                    text: i18n("Select a fan to continue.")
                }
            }

            // ============================================
            // STEP 1: Select Control Mode
            // ============================================
            ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing

                Kirigami.Heading {
                    text: i18n("Select Control Mode")
                    level: 3
                }

                Controls.Label {
                    text: i18n("Choose how the daemon should control this fan's output.")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: Kirigami.Theme.disabledTextColor
                }

                Controls.ComboBox {
                    id: controlModeCombo
                    Layout.fillWidth: true
                    Layout.maximumWidth: 360
                    model: availableControlModes()
                    currentIndex: {
                        var modes = availableControlModes()
                        var idx = modes.indexOf(selectedControlMode)
                        return idx >= 0 ? idx : 0
                    }
                    onActivated: {
                        selectedControlMode = model[currentIndex].toLowerCase()
                        console.log("KFC_GUI_DEBUG Wizard: controlMode changed to " + selectedControlMode)
                        draftModel.setControlModeViaDBus(selectedControlMode)
                    }
                }

                Controls.Label {
                    text: selectedControlMode === "pwm"
                        ? i18n("PWM (Pulse Width Modulation) — the most common fan control method.")
                        : i18n("Voltage control — adjusts fan speed by varying voltage.")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: Kirigami.Theme.disabledTextColor
                    font: Kirigami.Theme.smallFont
                }
            }

            // ============================================
            // STEP 2: Select Sensor Source(s)
            // ============================================
            ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing

                Kirigami.Heading {
                    text: i18n("Select Temperature Sensor(s)")
                    level: 3
                }

                Controls.Label {
                    text: i18n("Choose one or more temperature sensors as input for fan control. Select multiple sensors to use an aggregation function.")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: Kirigami.Theme.disabledTextColor
                }

                ListView {
                    id: sensorSelectionList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: 150
                    clip: true
                    model: sensorListModel

                    delegate: Kirigami.AbstractCard {
                        width: sensorSelectionList.width

                        contentItem: RowLayout {
                            spacing: Kirigami.Units.mediumSpacing

                            Controls.CheckBox {
                                id: sensorCheck
                                checked: selectedSensorIds.indexOf(model.sensorId) >= 0
                                onToggled: {
                                    var ids = selectedSensorIds.slice()
                                    var idx = ids.indexOf(model.sensorId)
                                    if (idx >= 0) {
                                        ids.splice(idx, 1)
                                    } else {
                                        ids.push(model.sensorId)
                                    }
                                    selectedSensorIds = ids
                                    console.log("KFC_GUI_DEBUG Wizard: sensor toggled sensorId=" + model.sensorId + " selectedSensorIds=" + JSON.stringify(ids))
                                    draftModel.setSensorIdsViaDBus(ids)
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0

                                Controls.Label {
                                    text: model.displayName
                                    font.weight: Font.DemiBold
                                }
                                Controls.Label {
                                    text: i18n("%1 °C — %2", (model.temperatureMillidegrees / 1000.0).toFixed(1), model.deviceName)
                                    font: Kirigami.Theme.smallFont
                                    color: Kirigami.Theme.disabledTextColor
                                }
                            }
                        }
                    }
                }

                Controls.Label {
                    text: selectedSensorIds.length === 0
                        ? i18n("Select at least one sensor to continue.")
                        : selectedSensorIds.length === 1
                        ? i18n("Single sensor selected.")
                        : i18n("%1 sensors selected — aggregation will be configured next.", selectedSensorIds.length)
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: selectedSensorIds.length === 0
                        ? Kirigami.Theme.negativeTextColor
                        : Kirigami.Theme.disabledTextColor
                }
            }

            // ============================================
            // STEP 3: Choose Aggregation (conditional)
            // ============================================
            ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing

                Kirigami.Heading {
                    text: i18n("Choose Aggregation")
                    level: 3
                }

                Controls.Label {
                    text: i18n("Choose how to combine the selected sensor readings into a single temperature input.")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: Kirigami.Theme.disabledTextColor
                }

                Controls.ComboBox {
                    id: aggregationCombo
                    Layout.fillWidth: true
                    Layout.maximumWidth: 360
                    model: ["Average", "Max", "Min", "Median"]
                    currentIndex: {
                        var idx = 0
                        if (selectedAggregation === "max") idx = 1
                        else if (selectedAggregation === "min") idx = 2
                        else if (selectedAggregation === "median") idx = 3
                        return idx
                    }
                    onActivated: {
                        var aggValues = ["average", "max", "min", "median"]
                        selectedAggregation = aggValues[currentIndex]
                        draftModel.setAggregationViaDBus(selectedAggregation)
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing

                    Controls.Label {
                        text: i18n("Average — mean of all selected sensor values")
                        font: Kirigami.Theme.smallFont
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.Label {
                        text: i18n("Max — highest sensor value (useful for worst-case cooling)")
                        font: Kirigami.Theme.smallFont
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.Label {
                        text: i18n("Min — lowest sensor value (conservative cooling target)")
                        font: Kirigami.Theme.smallFont
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.Label {
                        text: i18n("Median — middle value, reduces outlier influence")
                        font: Kirigami.Theme.smallFont
                        color: Kirigami.Theme.disabledTextColor
                    }
                }
            }

            // ============================================
            // STEP 4: Set Target Temperature
            // ============================================
            ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing

                Kirigami.Heading {
                    text: i18n("Set Target Temperature")
                    level: 3
                }

                Controls.Label {
                    text: i18n("The daemon will adjust fan output to keep the temperature near this target. Higher temperatures reduce fan speed but may risk overheating.")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: Kirigami.Theme.disabledTextColor
                }

                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 400

                    Controls.SpinBox {
                        Kirigami.FormData.label: i18n("Target temperature")
                        from: 200   // 20.0 °C
                        to: 1000     // 100.0 °C
                        stepSize: 5  // 0.5 °C steps
                        value: Math.round(selectedTargetTempCelsius * 10)

                        property int decimals: 1
                        textFromValue: function(v) { return (v / 10.0).toFixed(1) + " °C" }
                        valueFromText: function(text) {
                            var num = parseFloat(text)
                            return Math.round(num * 10)
                        }

                        onValueModified: {
                            selectedTargetTempCelsius = value / 10.0
                            draftModel.setTargetTempCelsiusViaDBus(selectedTargetTempCelsius)
                        }
                    }
                }

                Controls.Label {
                    text: i18n("Tip: A target of 65 °C is a reasonable starting point for most desktop fans.")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: Kirigami.Theme.disabledTextColor
                    font: Kirigami.Theme.smallFont
                }
            }

            // ============================================
            // STEP 5: Review Starter PID Values
            // ============================================
            ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing

                Kirigami.Heading {
                    text: i18n("Review Starter PID Values")
                    level: 3
                }

                Controls.Label {
                    text: i18n("These are the default PID tuning values. The daemon uses them to compute fan output based on the temperature error. You can adjust these later in the fan detail page.")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: Kirigami.Theme.disabledTextColor
                }

                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 400

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Kp")
                        text: draftModel.kp.toFixed(2)
                        font.weight: Font.DemiBold
                    }
                    Controls.Label {
                        text: i18n("Responds to the current temperature gap. Higher values react faster but can hunt.")
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        color: Kirigami.Theme.disabledTextColor
                        font: Kirigami.Theme.smallFont
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Ki")
                        text: draftModel.ki.toFixed(2)
                        font.weight: Font.DemiBold
                    }
                    Controls.Label {
                        text: i18n("Corrects steady offset over time. Higher values remove drift but can overshoot.")
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        color: Kirigami.Theme.disabledTextColor
                        font: Kirigami.Theme.smallFont
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Kd")
                        text: draftModel.kd.toFixed(2)
                        font.weight: Font.DemiBold
                    }
                    Controls.Label {
                        text: i18n("Damps fast temperature swings. Higher values can reduce overshoot but may amplify noise.")
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        color: Kirigami.Theme.disabledTextColor
                        font: Kirigami.Theme.smallFont
                    }
                }

                Controls.Label {
                    text: i18n("You can fine-tune these values later from the fan detail page, or use auto-tune to find optimal gains.")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: Kirigami.Theme.disabledTextColor
                    font: Kirigami.Theme.smallFont
                }
            }

            // ============================================
            // STEP 6: Review + Validate + Apply
            // ============================================
            ColumnLayout {
                spacing: Kirigami.Units.mediumSpacing

                Kirigami.Heading {
                    text: i18n("Review and Apply")
                    level: 3
                }

                Controls.Label {
                    text: i18n("Review your configuration before applying. Validate to check for errors, then apply to start daemon-managed control.")
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                    color: Kirigami.Theme.disabledTextColor
                }

                // Summary
                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 500

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Fan")
                        text: {
                            for (var i = 0; i < fanListModel.rowCount(); i++) {
                                var idx = fanListModel.index(i, 0)
                                if (fanListModel.data(idx, FanListModel.FanIdRole) === selectedFanId) {
                                    return fanListModel.data(idx, FanListModel.DisplayNameRole)
                                }
                            }
                            return selectedFanId
                        }
                        font.weight: Font.DemiBold
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Control mode")
                        text: selectedControlMode.toUpperCase()
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Sensor source(s)")
                        text: {
                            var names = []
                            for (var s = 0; s < selectedSensorIds.length; s++) {
                                var sId = selectedSensorIds[s]
                                for (var i = 0; i < sensorListModel.rowCount(); i++) {
                                    var idx = sensorListModel.index(i, 0)
                                    if (sensorListModel.data(idx, SensorListModel.SensorIdRole) === sId) {
                                        names.push(sensorListModel.data(idx, SensorListModel.DisplayNameRole))
                                        break
                                    }
                                }
                                if (names.length <= s) names.push(sId)  // fallback to ID
                            }
                            return names.join(", ")
                        }
                        wrapMode: Text.WordWrap
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Aggregation")
                        text: selectedSensorIds.length >= 2
                            ? selectedAggregation.charAt(0).toUpperCase() + selectedAggregation.slice(1)
                            : i18n("Single sensor")
                        visible: true
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Target temperature")
                        text: i18n("%1 °C", selectedTargetTempCelsius.toFixed(1))
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Kp")
                        text: draftModel.kp.toFixed(2)
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Ki")
                        text: draftModel.ki.toFixed(2)
                    }

                    Controls.Label {
                        Kirigami.FormData.label: i18n("Kd")
                        text: draftModel.kd.toFixed(2)
                    }
                }

                // Validation result
                Kirigami.InlineMessage {
                    id: wizardValidationSuccess
                    Layout.fillWidth: true
                    type: Kirigami.MessageType.Positive
                    visible: validationAttempted && !draftModel.hasValidationError
                    text: i18n("Valid: configuration checks passed")
                    showCloseButton: true
                }

                Kirigami.InlineMessage {
                    id: wizardValidationError
                    Layout.fillWidth: true
                    type: Kirigami.MessageType.Error
                    visible: draftModel.hasValidationError
                    text: i18n("Validation failed")

                    ColumnLayout {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        visible: draftModel.hasValidationError
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
                    id: wizardApplyResult
                    Layout.fillWidth: true
                    type: draftModel.hasApplyError ? Kirigami.MessageType.Warning : Kirigami.MessageType.Positive
                    visible: draftModel.hasApplyError || wizardDialog.applySucceeded
                    text: draftModel.hasApplyError
                        ? i18n("Some fans were rejected during apply")
                        : i18n("Configuration applied successfully. This fan is now under daemon control.")

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
            }
        }

        // Navigation buttons
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.mediumSpacing

            Controls.Button {
                text: i18n("Cancel")
                icon.name: "dialog-cancel-symbolic"
                onClicked: cancelWizard()
            }

            Item { Layout.fillWidth: true }

            Controls.Button {
                text: i18n("Back")
                icon.name: "go-previous-symbolic"
                visible: currentStep > 0 && currentStep < wizardDialog.stepReview
                enabled: currentStep > 0
                onClicked: goBack()
            }

            Controls.Button {
                text: i18n("Next")
                icon.name: "go-next-symbolic"
                visible: currentStep < wizardDialog.stepReview
                enabled: {
                    if (currentStep === wizardDialog.stepFan) return selectedFanId !== ""
                    if (currentStep === wizardDialog.stepSensor) return selectedSensorIds.length > 0
                    return true
                }
                highlighted: true
                onClicked: {
                    // On step transitions that need data flushes
                    if (currentStep === wizardDialog.stepFan && !draftLoaded) {
                        loadFanIntoDraft(selectedFanId)
                    }
                    if (currentStep === wizardDialog.stepControlMode) {
                        // Control mode already flushed on selection
                    }
                    if (currentStep === wizardDialog.stepSensor && selectedSensorIds.length === 1) {
                        // Auto-set aggregation to "single" concept
                        draftModel.setAggregationViaDBus("average")
                    }
                    goNext()
                }
            }

            Controls.Button {
                text: i18n("Validate Draft")
                icon.name: "checkmark-symbolic"
                visible: currentStep === wizardDialog.stepReview
                enabled: statusMonitor.daemonConnected
                onClicked: {
                    validationAttempted = true
                    draftModel.validateDraft()
                }
            }

            Controls.Button {
                text: i18n("Apply Changes")
                icon.name: "dialog-ok-apply-symbolic"
                visible: currentStep === wizardDialog.stepReview
                highlighted: true
                enabled: statusMonitor.daemonConnected
                onClicked: {
                    validationAttempted = true
                    applyAttempted = true
                    draftModel.applyDraft()
                    // After apply, if no errors, close the wizard
                    // The apply result will show in the banner
                }
            }
        }
    }

    // Track apply success to show result before closing
    property bool applySucceeded: false

    Connections {
        target: draftModel
        function onApplyStateChanged() {
            console.log("KFC_GUI_DEBUG Wizard: onApplyStateChanged hasApplyError=" + draftModel.hasApplyError + " applyAttempted=" + applyAttempted)
            if (applyAttempted && !draftModel.hasApplyError && draftModel.applyErrors.length === 0) {
                // Apply succeeded — schedule close after brief display
                wizardDialog.applySucceeded = true
                closeTimer.start()
            } else {
                wizardDialog.applySucceeded = false
            }
        }
    }

    Timer {
        id: closeTimer
        interval: 2000
        onTriggered: {
            wizardDialog.close()
        }
    }

    // Handle close: discard draft if not applied
    onClosed: {
        console.log("KFC_GUI_DEBUG Wizard: onClosed applySucceeded=" + applySucceeded + " draftLoaded=" + draftLoaded)
        if (!applySucceeded && draftLoaded) {
            // If the user closed without applying, discard any draft
            // (but not if apply succeeded — the draft was promoted to applied config)
            draftModel.discardDraft()
        }
        applySucceeded = false
        validationAttempted = false
        applyAttempted = false
    }
}
