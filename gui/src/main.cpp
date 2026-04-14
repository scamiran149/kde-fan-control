/*
 * KDE Fan Control — GUI Application Entry Point
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include <QApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QIcon>
#include <QDebug>
#include <QWindow>

#include <klocalizedqmlcontext.h>

#include "daemon_interface.h"
#include "status_monitor.h"
#include "models/fan_list_model.h"
#include "models/sensor_list_model.h"
#include "models/draft_model.h"
#include "models/lifecycle_event_model.h"
#include "models/overview_model.h"
#include "models/overview_fan_row.h"
#include "tray_icon.h"
#include "notification_handler.h"

int main(int argc, char *argv[])
{
    QApplication app(argc, argv);
    app.setOrganizationName(QStringLiteral("KDE"));
    app.setOrganizationDomain(QStringLiteral("org.kde"));
    app.setApplicationName(QStringLiteral("kde-fan-control-gui"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));
    app.setWindowIcon(QIcon::fromTheme(QStringLiteral("kde-fan-control")));

    // Central DBus proxy for the daemon on the system bus.
    DaemonInterface daemonInterface;

    // Reactive model updates driven by DBus signals.
    FanListModel fanListModel;
    SensorListModel sensorListModel;
    OverviewModel overviewModel;
    DraftModel draftModel(&daemonInterface);
    LifecycleEventModel lifecycleEventModel;
    StatusMonitor statusMonitor(&daemonInterface, &fanListModel, &sensorListModel, &overviewModel);

    // System tray icon and notification handler (Plan 03).
    TrayIcon trayIcon(&statusMonitor, &overviewModel);
    NotificationHandler notificationHandler(&statusMonitor, &overviewModel);

    // Wire up initial data loading: once the status monitor detects
    // the daemon is alive it will trigger the model refreshes.
    QMetaObject::invokeMethod(&statusMonitor, &StatusMonitor::checkDaemonConnected,
                              Qt::QueuedConnection);

    QQmlApplicationEngine engine;
    auto *localizedContext = KLocalization::setupLocalizedContext(&engine);
    localizedContext->setTranslationDomain(QStringLiteral("kde-fan-control"));

    // Register enum types for QML access.
    qmlRegisterUncreatableType<FanListModel>("org.kde.fancontrol", 1, 0,
        "FanListModel", QStringLiteral("Cannot create FanListModel in QML"));
    qmlRegisterUncreatableType<SensorListModel>("org.kde.fancontrol", 1, 0,
        "SensorListModel", QStringLiteral("Cannot create SensorListModel in QML"));
    qmlRegisterUncreatableType<OverviewModel>("org.kde.fancontrol", 1, 0,
        "OverviewModel", QStringLiteral("Cannot create OverviewModel in QML"));

    // Register OverviewFanRow so QML can access rowObject properties directly.
    qmlRegisterUncreatableType<OverviewFanRow>("org.kde.fancontrol", 1, 0,
        "OverviewFanRow", QStringLiteral("Cannot create OverviewFanRow in QML"));

    // Register context properties for QML access.
    engine.rootContext()->setContextProperty(QStringLiteral("daemonInterface"), &daemonInterface);
    engine.rootContext()->setContextProperty(QStringLiteral("statusMonitor"), &statusMonitor);
    engine.rootContext()->setContextProperty(QStringLiteral("fanListModel"), &fanListModel);
    engine.rootContext()->setContextProperty(QStringLiteral("sensorListModel"), &sensorListModel);
    engine.rootContext()->setContextProperty(QStringLiteral("overviewModel"), &overviewModel);
    engine.rootContext()->setContextProperty(QStringLiteral("draftModel"), &draftModel);
    engine.rootContext()->setContextProperty(QStringLiteral("lifecycleEventModel"), &lifecycleEventModel);
    engine.rootContext()->setContextProperty(QStringLiteral("trayIcon"), &trayIcon);
    engine.rootContext()->setContextProperty(QStringLiteral("notificationHandler"), &notificationHandler);

    // Load the main QML file from the QML module resource.
    const QUrl url(QStringLiteral("qrc:/org/kde/fancontrol/qml/Main.qml"));

    QObject::connect(&engine, &QQmlApplicationEngine::objectCreated,
                     &app, [url, &trayIcon](QObject *obj, const QUrl &objUrl) {
        if (!obj && url == objUrl) {
            qCritical() << "Failed to create QML root object from" << url;
            QCoreApplication::exit(-1);
        }
        if (obj) {
            if (auto *window = qobject_cast<QWindow *>(obj)) {
                trayIcon.setAssociatedWindow(window);
            }
        }
    }, Qt::QueuedConnection);

    engine.load(url);

    return app.exec();
}
