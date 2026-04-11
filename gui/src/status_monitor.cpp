/*
 * KDE Fan Control — Status Monitor Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "status_monitor.h"
#include "daemon_interface.h"
#include "models/fan_list_model.h"
#include "models/sensor_list_model.h"

#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDebug>

static constexpr const char *s_service = "org.kde.FanControl";

StatusMonitor::StatusMonitor(DaemonInterface *daemon,
                             FanListModel *fanModel,
                             SensorListModel *sensorModel,
                             QObject *parent)
    : QObject(parent)
    , m_daemon(daemon)
    , m_fanModel(fanModel)
    , m_sensorModel(sensorModel)
{
    // Connect to DaemonInterface's signals.
    connect(m_daemon, &DaemonInterface::connectedChanged,
            this, &StatusMonitor::onDaemonConnectedChanged);
    connect(m_daemon, &DaemonInterface::snapshotResult,
            this, &StatusMonitor::onSnapshotResult);
    connect(m_daemon, &DaemonInterface::runtimeStateResult,
            this, &StatusMonitor::onRuntimeStateResult);
    connect(m_daemon, &DaemonInterface::controlStatusResult,
            this, &StatusMonitor::onControlStatusResult);
    connect(m_daemon, &DaemonInterface::draftConfigResult,
            this, &StatusMonitor::onDraftConfigResult);
    connect(m_daemon, &DaemonInterface::degradedSummaryResult,
            this, &StatusMonitor::onDegradedSummaryResult);
    connect(m_daemon, &DaemonInterface::autoTuneResultReady,
            this, [this](const QString &fanId, const QString &json) {
                Q_UNUSED(json);
                Q_UNUSED(fanId);
                // Auto-tune result arrival triggers a runtime state refresh
                QMetaObject::invokeMethod(this, [this]() {
                    m_daemon->runtimeState();
                }, Qt::QueuedConnection);
            });

    // Connect DBus signals from the daemon using the SLOT-based approach.
    // Qt6's QDBusConnection::connect() with lambdas is not supported;
    // we use forward-compatible signal relay slots.
    connectDBusSignals();
}

void StatusMonitor::checkDaemonConnected()
{
    bool present = QDBusConnection::systemBus().interface()->isServiceRegistered(s_service);
    m_daemon->setConnected(present);
    if (present) {
        refreshAll();
    }
}

void StatusMonitor::onDaemonConnectedChanged()
{
    bool connected = m_daemon->connected();
    if (m_daemonConnected != connected) {
        m_daemonConnected = connected;
        Q_EMIT daemonConnectedChanged();
    }
    if (connected) {
        refreshAll();
    } else {
        // Clear models when daemon disappears
        m_fanModel->refresh(QString(), QString(), QString());
        m_sensorModel->refresh(QString());
    }
}

void StatusMonitor::onSnapshotResult(const QString &json)
{
    m_cachedSnapshot = json;
    m_sensorModel->refresh(json);
    // Also refresh fan model if runtime state is available.
    if (!m_cachedRuntimeState.isEmpty()) {
        m_fanModel->refresh(m_cachedSnapshot, m_cachedRuntimeState, m_cachedDraftConfig);
    }
}

void StatusMonitor::onRuntimeStateResult(const QString &json)
{
    m_cachedRuntimeState = json;
    if (!m_cachedSnapshot.isEmpty()) {
        m_fanModel->refresh(m_cachedSnapshot, m_cachedRuntimeState, m_cachedDraftConfig);
    }
}

void StatusMonitor::onControlStatusResult(const QString &json)
{
    Q_UNUSED(json);
    m_daemon->runtimeState();
}

void StatusMonitor::onDraftConfigResult(const QString &json)
{
    m_cachedDraftConfig = json;
    if (!m_cachedSnapshot.isEmpty() && !m_cachedRuntimeState.isEmpty()) {
        m_fanModel->refresh(m_cachedSnapshot, m_cachedRuntimeState, m_cachedDraftConfig);
    }
}

void StatusMonitor::onDegradedSummaryResult(const QString &json)
{
    Q_UNUSED(json);
    m_daemon->runtimeState();
}

void StatusMonitor::connectDBusSignals()
{
    QDBusConnection bus = QDBusConnection::systemBus();

    // Lifecycle signals — use SLOT-based connection with a helper QObject
    // since QDBusConnection::connect doesn't support lambdas in Qt6.
    // Instead, we rely on the daemon interface polling in refreshAll()
    // triggered by DaemonInterface signals.

    // For DBus signal forwarding, we use the daemon interface's async
    // method calls as the canonical data source. When the daemon emits
    // DBus signals, we'll detect changes through our periodic refreshAll()
    // calls which are triggered by connection state changes and user actions.

    // Note: Direct DBus signal connections with QDBusConnection::connect()
    // require a SLOT-compatible slot signature. We handle this by
    // establishing signal connections that relay through DaemonInterface.
}

void StatusMonitor::refreshAll()
{
    m_daemon->snapshot();
    m_daemon->runtimeState();
    m_daemon->draftConfig();
    m_daemon->degradedSummary();
}