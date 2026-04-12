/*
 * KDE Fan Control — Main QML Application Window
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.fancontrol

Kirigami.ApplicationWindow {
    id: root
    title: i18n("Fan Control")
    width: 1180
    height: 820
    minimumWidth: 980
    minimumHeight: 700

    globalDrawer: Kirigami.GlobalDrawer {
        isMenu: true
        actions: [
            Kirigami.Action {
                text: i18n("Overview")
                iconName: "go-home"
                onTriggered: { pageStack.clear(); pageStack.push(overviewPage) }
            },
            Kirigami.Action {
                text: i18n("Inventory")
                iconName: "view-list-symbolic"
                onTriggered: { pageStack.clear(); pageStack.push(inventoryPage) }
            },
            Kirigami.Action {
                separator: true
            },
            Kirigami.Action {
                text: i18n("About")
                iconName: "help-about"
            },
            Kirigami.Action {
                text: i18n("Quit")
                iconName: "application-exit"
                onTriggered: Qt.quit()
            }
        ]
    }

    OverviewPage {
        id: overviewPage
    }

    InventoryPage {
        id: inventoryPage
    }

    // Wizard configuration dialog (Plan 04)
    WizardDialog {
        id: wizardDialog
    }

    pageStack.initialPage: overviewPage

    Connections {
        target: daemonInterface
        function onSnapshotResult(json) {
            statusMonitor.onSnapshotResult(json)
        }
        function onRuntimeStateResult(json) {
            statusMonitor.onRuntimeStateResult(json)
        }
        function onControlStatusResult(json) {
            statusMonitor.onControlStatusResult(json)
        }
    }
}