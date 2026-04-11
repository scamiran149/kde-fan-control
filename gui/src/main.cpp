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

#include "daemon_interface.h"
#include "status_monitor.h"
#include "models/fan_list_model.h"
#include "models/sensor_list_model.h"
#include "models/draft_model.h"
#include "models/lifecycle_event_model.h"

int main(int argc, char *argv[])
{
    QApplication app(argc, argv);
    app.setOrganizationName(QStringLiteral("KDE"));
    app.setOrganizationDomain(QStringLiteral("org.kde"));
    app.setApplicationName(QStringLiteral("kde-fan-control-gui"));
    app.setApplicationVersion(QStringLiteral("0.1.0"));

    // Central DBus proxy for the daemon on the system bus.
    DaemonInterface daemonInterface;

    // Reactive model updates driven by DBus signals.
    FanListModel fanListModel;
    SensorListModel sensorListModel;
    DraftModel draftModel(&daemonInterface);
    LifecycleEventModel lifecycleEventModel;
    StatusMonitor statusMonitor(&daemonInterface, &fanListModel, &sensorListModel);

    // Wire up initial data loading: once the status monitor detects
    // the daemon is alive it will trigger the model refreshes.
    QMetaObject::invokeMethod(&statusMonitor, &StatusMonitor::checkDaemonConnected,
                              Qt::QueuedConnection);

    QQmlApplicationEngine engine;

    // Register context properties for QML access.
    engine.rootContext()->setContextProperty(QStringLiteral("daemonInterface"), &daemonInterface);
    engine.rootContext()->setContextProperty(QStringLiteral("statusMonitor"), &statusMonitor);
    engine.rootContext()->setContextProperty(QStringLiteral("fanListModel"), &fanListModel);
    engine.rootContext()->setContextProperty(QStringLiteral("sensorListModel"), &sensorListModel);
    engine.rootContext()->setContextProperty(QStringLiteral("draftModel"), &draftModel);
    engine.rootContext()->setContextProperty(QStringLiteral("lifecycleEventModel"), &lifecycleEventModel);

    // Load the main QML file from the QML module resource.
    const QUrl url(QStringLiteral("qrc:/qt/qml/org/kde/fancontrol/qml/Main.qml"));

    QObject::connect(&engine, &QQmlApplicationEngine::objectCreated,
                     &app, [url](QObject *obj, const QUrl &objUrl) {
        if (!obj && url == objUrl) {
            qCritical() << "Failed to create QML root object from" << url;
            QCoreApplication::exit(-1);
        }
    }, Qt::QueuedConnection);

    engine.load(url);

    return app.exec();
}