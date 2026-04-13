/*
 * KDE Fan Control — Rename Dialog
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Dialog for assigning or removing a friendly name on a sensor or fan.
 * Operates through the daemon DBus Inventory interface.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Controls.Dialog {
    id: renameDialog

    property string itemId: ""
    property string itemType: "fan"
    property string currentFriendlyName: ""
    property string hardwareLabel: ""
    property bool _pendingWrite: false

    title: itemType === "sensor" ? i18n("Rename Sensor") : i18n("Rename Fan")

    modal: true
    focus: true
    closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside

    x: Math.round((parent.width - width) / 2)
    y: Math.round((parent.height - height) / 2)
    width: Math.min(parent ? parent.width * 0.6 : 420, 420)

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.mediumSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: i18n("Assign a friendly name to make this %1 easier to identify. The hardware label will still be shown as a subtitle.", renameDialog.itemType)
            wrapMode: Text.WordWrap
            color: Kirigami.Theme.disabledTextColor
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.Label {
                Kirigami.FormData.label: i18n("Hardware ID")
                text: renameDialog.itemId
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.WrapAnywhere
                Layout.fillWidth: true
            }

            Controls.Label {
                Kirigami.FormData.label: i18n("Hardware label")
                text: renameDialog.hardwareLabel || i18n("(none)")
                color: Kirigami.Theme.disabledTextColor
                Layout.fillWidth: true
            }

            Controls.TextField {
                id: nameField
                Kirigami.FormData.label: i18n("Friendly name")
                Layout.fillWidth: true
                placeholderText: renameDialog.hardwareLabel || i18n("Enter a friendly name")
                text: renameDialog.currentFriendlyName
                enabled: !renameDialog._pendingWrite
                onAccepted: renameDialog.acceptRename()
            }
        }
    }

    footer: Controls.DialogButtonBox {
        Controls.Button {
            text: i18n("Remove Name")
            icon.name: "edit-delete-remove"
            visible: renameDialog.currentFriendlyName.length > 0
            enabled: !renameDialog._pendingWrite && renameDialog.currentFriendlyName.length > 0 && statusMonitor.daemonConnected && daemonInterface.canWrite
            onClicked: renameDialog.removeName()
            palette.buttonText: Kirigami.Theme.negativeTextColor
        }

        Controls.Button {
            text: i18n("Cancel")
            icon.name: "dialog-cancel"
            enabled: !renameDialog._pendingWrite
            onClicked: renameDialog.reject()
        }

        Controls.Button {
            text: i18n("Save")
            icon.name: "dialog-ok-apply"
            highlighted: true
            enabled: !renameDialog._pendingWrite && statusMonitor.daemonConnected && daemonInterface.canWrite && nameField.text.trim() !== renameDialog.currentFriendlyName
            onClicked: renameDialog.acceptRename()
        }
    }

    Connections {
        target: daemonInterface

        function onWriteSucceeded(method) {
            if (!renameDialog._pendingWrite) return
            if (method === "setSensorName" || method === "setFanName"
                || method === "removeSensorName" || method === "removeFanName") {
                renameDialog._pendingWrite = false
                daemonInterface.snapshot()
                renameDialog.close()
            }
        }

        function onWriteFailed(method, error) {
            if (!renameDialog._pendingWrite) return
            if (method === "setSensorName" || method === "setFanName"
                || method === "removeSensorName" || method === "removeFanName") {
                renameDialog._pendingWrite = false
            }
        }
    }

    function acceptRename() {
        var newName = nameField.text.trim()
        if (newName === renameDialog.currentFriendlyName) {
            return
        }
        renameDialog._pendingWrite = true
        if (renameDialog.itemType === "sensor") {
            daemonInterface.setSensorName(renameDialog.itemId, newName)
        } else {
            daemonInterface.setFanName(renameDialog.itemId, newName)
        }
    }

    function removeName() {
        renameDialog._pendingWrite = true
        if (renameDialog.itemType === "sensor") {
            daemonInterface.removeSensorName(renameDialog.itemId)
        } else {
            daemonInterface.removeFanName(renameDialog.itemId)
        }
    }

    function openFor(itemType, itemId, currentFriendlyName, hardwareLabel) {
        renameDialog.itemType = itemType
        renameDialog.itemId = itemId
        renameDialog.currentFriendlyName = currentFriendlyName
        renameDialog.hardwareLabel = hardwareLabel || ""
        renameDialog._pendingWrite = false
        nameField.text = currentFriendlyName
        renameDialog.open()
    }

    onOpened: {
        nameField.forceActiveFocus()
        nameField.selectAll()
    }

    onRejected: {
        renameDialog._pendingWrite = false
    }
}