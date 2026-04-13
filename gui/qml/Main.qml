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

    onVisibilityChanged: {
        if (root.visibility === Window.Hidden || root.visibility === Window.Minimized) {
            statusMonitor.pollingEnabled = false
        } else {
            statusMonitor.pollingEnabled = true
        }
    }

    header: Kirigami.ActionToolBar {
        actions: [
            Kirigami.Action {
                id: toolbarAuthAction
                text: daemonInterface.canWrite ? i18n("Lock") : i18n("Unlock")
                icon.name: daemonInterface.canWrite ? "object-locked" : "object-unlocked"
                visible: statusMonitor.daemonConnected
                onTriggered: {
                    if (daemonInterface.canWrite) {
                        daemonInterface.dropAuthorization()
                    } else {
                        daemonInterface.requestAuthorization()
                    }
                }
            }
        ]
    }

    globalDrawer: Kirigami.GlobalDrawer {
        isMenu: true
        actions: [
            Kirigami.Action {
                text: i18n("Overview")
                icon.name: "go-home"
                onTriggered: { pageStack.clear(); pageStack.push(overviewPage) }
            },
            Kirigami.Action {
                text: i18n("Inventory")
                icon.name: "view-list-symbolic"
                onTriggered: { pageStack.clear(); pageStack.push(inventoryPage) }
            },
            Kirigami.Action {
                separator: true
            },
            Kirigami.Action {
                id: authAction
                text: daemonInterface.canWrite ? i18n("Lock") : i18n("Unlock")
                icon.name: daemonInterface.canWrite ? "object-locked" : "object-unlocked"
                visible: statusMonitor.daemonConnected
                onTriggered: {
                    if (daemonInterface.canWrite) {
                        daemonInterface.dropAuthorization()
                    } else {
                        daemonInterface.requestAuthorization()
                    }
                }
            },
            Kirigami.Action {
                separator: true
            },
            Kirigami.Action {
                text: i18n("About")
                icon.name: "help-about"
            },
            Kirigami.Action {
                text: i18n("Quit")
                icon.name: "application-exit"
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

    // Placeholder component for future tray popover wiring.
    // Keep it hidden so it doesn't render inside the main window.
    TrayPopover {
        id: trayPopover
        visible: false
    }

    pageStack.initialPage: overviewPage

    Connections {
        target: trayIcon
        function onActivateMainWindow() {
            root.show()
            root.raise()
            root.requestActivate()
        }
    }
}
