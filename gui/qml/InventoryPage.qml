/*
 * KDE Fan Control — Inventory Page
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Read-first diagnostic view of all discovered sensors and fans.
 */

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Kirigami.ScrollablePage {
    id: inventoryPage
    title: i18n("Inventory")

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        width: inventoryPage.width

        // Sensors section
        Kirigami.AbstractCard {
            Layout.fillWidth: true
            visible: sensorListModel.rowCount() > 0

            header: Kirigami.Heading {
                text: i18n("Sensors")
                level: 2
            }

            ColumnLayout {
                id: sensorList
                spacing: Kirigami.Units.smallSpacing

                Repeater {
                    model: sensorListModel

                    delegate: Kirigami.AbstractCard {
                        Layout.fillWidth: true
                        contentItem: RowLayout {
                            spacing: Kirigami.Units.mdSpacing

                            Kirigami.TitleSubtitle {
                                title: model.displayName
                                subtitle: model.sensorId
                                Layout.fillWidth: true
                            }

                            Kirigami.TitleSubtitle {
                                title: {
                                    var mdeg = model.temperatureMillidegrees;
                                    return mdeg > 0 ? (mdeg / 1000.0).toFixed(1) + " °C" : "—";
                                }
                                subtitle: model.deviceName
                            }
                        }
                    }
                }
            }
        }

        // Fans section
        Kirigami.AbstractCard {
            Layout.fillWidth: true
            visible: fanListModel.rowCount() > 0

            header: Kirigami.Heading {
                text: i18n("Fans")
                level: 2
            }

            ColumnLayout {
                id: fanList
                spacing: Kirigami.Units.smallSpacing

                Repeater {
                    model: fanListModel

                    delegate: Kirigami.AbstractCard {
                        Layout.fillWidth: true
                        contentItem: RowLayout {
                            spacing: Kirigami.Units.mdSpacing

                            Kirigami.TitleSubtitle {
                                title: model.displayName
                                subtitle: model.fanId
                                Layout.fillWidth: true
                            }

                            StateBadge {
                                fanState: model.state
                                highTempAlert: model.highTempAlert
                            }

                            ColumnLayout {
                                spacing: 0

                                Controls.Label {
                                    text: {
                                        var modes = model.controlMode;
                                        return modes ? modes : i18n("No control mode");
                                    }
                                    font: Kirigami.Theme.smallFont
                                }

                                Controls.Label {
                                    text: model.hasTach ? i18n("Has tach") : i18n("No tach")
                                    font: Kirigami.Theme.smallFont
                                    color: model.hasTach ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.disabledTextColor
                                }

                                Controls.Label {
                                    text: model.supportReason || ""
                                    visible: model.supportReason.length > 0
                                    font: Kirigami.Theme.smallFont
                                    color: Kirigami.Theme.disabledTextColor
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}