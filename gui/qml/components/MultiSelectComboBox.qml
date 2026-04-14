/*
 * KDE Fan Control — Multi-Select Combo Box
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Reusable dropdown component with checkboxes for multi-selection.
 * Displays selected item names in the trigger button (truncated with
 * ellipsis if too long). Opens a popup with a scrollable list of
 * checkable items on click.
 *
 * Properties:
 *   - listModel:        the QAbstractListModel to display
 *   - idRole:           role name string for the unique identifier
 *   - displayRole:      role name string for display text
 *   - detailRole:       role name string for secondary detail (optional)
 *   - detailFormatter:  function(value) → string, for formatting detail
 *                        values in delegate text (optional; defaults to
 *                        String(rawValue))
 *   - selectedIds:      var array of currently-selected IDs
 *   - enabled:          whether the control is interactive
 *   - placeholderText:  text shown when nothing is selected
 *
 * Signals:
 *   - selectionChanged(var newIds): emitted when the selection changes
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.Button {
    id: root

    property alias listModel: itemView.model
    property string idRole: "sensorId"
    property string displayRole: "displayName"
    property string detailRole: ""
    property var detailFormatter: null
    property var selectedIds: []
    property string placeholderText: i18n("Select items...")
    property string summaryText: ""

    signal selectionChanged(var newIds)

    function _formatDetail(rawValue) {
        if (root.detailFormatter) return root.detailFormatter(rawValue)
        return String(rawValue)
    }

    implicitWidth: 320
    implicitHeight: Kirigami.Units.gridUnit * 2
    leftPadding: Kirigami.Units.smallSpacing
    rightPadding: Kirigami.Units.smallSpacing
    topPadding: Kirigami.Units.smallSpacing
    bottomPadding: Kirigami.Units.smallSpacing

    contentItem: RowLayout {
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: {
                var label = root.summaryText !== "" ? root.summaryText : root.placeholderText
                if (label.length > 72) {
                    label = label.substring(0, 71) + "..."
                }
                return label
            }
            color: root.selectedIds.length === 0
                ? Kirigami.Theme.disabledTextColor
                : Kirigami.Theme.textColor
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
        }

        Kirigami.Icon {
            source: "pan-down-symbolic"
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Kirigami.Units.iconSizes.small
            Layout.preferredHeight: Kirigami.Units.iconSizes.small
            color: Kirigami.Theme.textColor
        }
    }

    onClicked: {
        if (dropdownPopup.opened) {
            dropdownPopup.close()
        } else {
            dropdownPopup.open()
        }
    }

    Controls.ToolTip.visible: hovered && root.summaryText !== ""
    Controls.ToolTip.text: root.summaryText
    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay

    Controls.Popup {
        id: dropdownPopup
        y: root.height
        x: 0
        width: root.width
        padding: Kirigami.Units.smallSpacing

        contentItem: ListView {
            id: itemView
            implicitHeight: Math.min(contentHeight, 520)
            clip: true

            Controls.ScrollBar.vertical: Controls.ScrollBar {
                policy: Controls.ScrollBar.AsNeeded
            }

            delegate: Controls.CheckBox {
                width: itemView.width - (itemView.Controls.ScrollBar.vertical.visible ? itemView.Controls.ScrollBar.vertical.width : 0)
                text: {
                    var display = model[root.displayRole] || ""
                    if (root.detailRole !== "") {
                        var detail = model[root.detailRole]
                        if (detail !== undefined && detail !== "") {
                            return display + " (" + root._formatDetail(detail) + ")"
                        }
                    }
                    return display
                }
                checked: root.selectedIds.indexOf(model[root.idRole]) >= 0
                onToggled: {
                    var ids = root.selectedIds.slice()
                    var idx = ids.indexOf(model[root.idRole])
                    if (idx >= 0) {
                        ids.splice(idx, 1)
                    } else {
                        ids.push(model[root.idRole])
                    }
                    root.selectedIds = ids
                    root.selectionChanged(ids)
                }

                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: text
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }

            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.largeSpacing * 2
                visible: itemView.model && itemView.model.rowCount() === 0
                text: i18n("No items available")
            }
        }
    }
}
