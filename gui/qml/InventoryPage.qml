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

        Kirigami.Heading {
            text: i18n("Sensors")
            level: 2
            visible: sensorRepeater.count > 0
        }

        Repeater {
            id: sensorRepeater
            model: sensorListModel

            delegate: Kirigami.AbstractCard {
                Layout.fillWidth: true
                contentItem: RowLayout {
                    spacing: Kirigami.Units.mediumSpacing

                    Kirigami.TitleSubtitle {
                        title: displayName
                        subtitle: sensorId
                        Layout.fillWidth: true
                    }

                    Kirigami.TitleSubtitle {
                        title: {
                            var mdeg = temperatureMillidegrees;
                            return mdeg > 0 ? (mdeg / 1000.0).toFixed(1) + " °C" : "—";
                        }
                        subtitle: deviceName
                    }
                }
            }
        }

        Kirigami.Heading {
            text: i18n("Fans")
            level: 2
            visible: fanRepeater.count > 0
        }

        Repeater {
            id: fanRepeater
            model: fanListModel

            delegate: Kirigami.AbstractCard {
                Layout.fillWidth: true
                contentItem: RowLayout {
                    spacing: Kirigami.Units.mediumSpacing

                    Kirigami.TitleSubtitle {
                        title: displayName
                        subtitle: fanId
                        Layout.fillWidth: true
                    }

                    StateBadge {
                        fanState: model.state
                        highTempAlert: highTempAlert
                    }

                    ColumnLayout {
                        spacing: 0

                        Controls.Label {
                            text: {
                                var modes = controlMode;
                                return modes ? modes : i18n("No control mode");
                            }
                            font: Kirigami.Theme.smallFont
                        }

                        Controls.Label {
                            text: hasTach ? i18n("Has tach") : i18n("No tach")
                            font: Kirigami.Theme.smallFont
                            color: hasTach ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.disabledTextColor
                        }

                        Controls.Label {
                            text: supportReason || ""
                            visible: supportReason.length > 0
                            font: Kirigami.Theme.smallFont
                            color: Kirigami.Theme.disabledTextColor
                        }
                    }
                }
            }
        }
    }
}
