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
    connectDBusSignals();

    // 3-second polling timer for live data updates.
    m_refreshTimer = new QTimer(this);
    m_refreshTimer->setInterval(3000);
    m_refreshTimer->setSingleShot(false);
    connect(m_refreshTimer, &QTimer::timeout, this, &StatusMonitor::refreshAll);

    // Sync the initial daemon state from the proxy so QML doesn't start in a
    // disconnected state while successful DBus calls are already flowing.
    onDaemonConnectedChanged();
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
        if (m_pollingEnabled) {
            m_refreshTimer->start();
        }
    } else {
        m_refreshTimer->stop();
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
        m_fanModel->refresh(m_cachedSnapshot, m_cachedRuntimeState, m_cachedDraftConfig, m_cachedControlStatus);
    }
}

void StatusMonitor::onRuntimeStateResult(const QString &json)
{
    m_cachedRuntimeState = json;
    if (!m_cachedSnapshot.isEmpty()) {
        m_fanModel->refresh(m_cachedSnapshot, m_cachedRuntimeState, m_cachedDraftConfig, m_cachedControlStatus);
    }
}

void StatusMonitor::onControlStatusResult(const QString &json)
{
    m_cachedControlStatus = json;
    if (!m_cachedSnapshot.isEmpty() && !m_cachedRuntimeState.isEmpty()) {
        m_fanModel->refresh(m_cachedSnapshot, m_cachedRuntimeState, m_cachedDraftConfig, m_cachedControlStatus);
    }
}

void StatusMonitor::onDraftConfigResult(const QString &json)
{
    m_cachedDraftConfig = json;
    if (!m_cachedSnapshot.isEmpty() && !m_cachedRuntimeState.isEmpty()) {
        m_fanModel->refresh(m_cachedSnapshot, m_cachedRuntimeState, m_cachedDraftConfig, m_cachedControlStatus);
    }
}

void StatusMonitor::onDegradedSummaryResult(const QString &json)
{
    Q_UNUSED(json);
    m_daemon->runtimeState();
}

void StatusMonitor::onDaemonDisconnected()
{
    m_refreshTimer->stop();
    m_daemonConnected = false;
    Q_EMIT daemonConnectedChanged();
    m_fanModel->refresh(QString(), QString(), QString());
    m_sensorModel->refresh(QString());
    m_cachedSnapshot.clear();
    m_cachedRuntimeState.clear();
    m_cachedDraftConfig.clear();
    m_cachedControlStatus.clear();
}

void StatusMonitor::setPollingEnabled(bool enabled)
{
    if (m_pollingEnabled == enabled)
        return;
    m_pollingEnabled = enabled;
    Q_EMIT pollingEnabledChanged();
    if (enabled && m_daemonConnected) {
        m_refreshTimer->start();
    } else {
        m_refreshTimer->stop();
    }
}

void StatusMonitor::connectDBusSignals()
{
    QDBusConnection bus = QDBusConnection::systemBus();

    bus.connect(QStringLiteral("org.kde.FanControl"),
                QStringLiteral("/org/kde/FanControl/Lifecycle"),
                QStringLiteral("org.kde.FanControl.Lifecycle"),
                QStringLiteral("DraftChanged"),
                m_daemon, SLOT(onDraftChanged()));

    bus.connect(QStringLiteral("org.kde.FanControl"),
                QStringLiteral("/org/kde/FanControl/Lifecycle"),
                QStringLiteral("org.kde.FanControl.Lifecycle"),
                QStringLiteral("AppliedConfigChanged"),
                m_daemon, SLOT(onAppliedConfigChanged()));

    bus.connect(QStringLiteral("org.kde.FanControl"),
                QStringLiteral("/org/kde/FanControl/Lifecycle"),
                QStringLiteral("org.kde.FanControl.Lifecycle"),
                QStringLiteral("DegradedStateChanged"),
                m_daemon, SLOT(onDegradedStateChanged()));

    bus.connect(QStringLiteral("org.kde.FanControl"),
                QStringLiteral("/org/kde/FanControl/Lifecycle"),
                QStringLiteral("org.kde.FanControl.Lifecycle"),
                QStringLiteral("LifecycleEventAppended"),
                m_daemon, SLOT(onLifecycleEventAppended(QString,QString)));

    bus.connect(QStringLiteral("org.kde.FanControl"),
                QStringLiteral("/org/kde/FanControl/Control"),
                QStringLiteral("org.kde.FanControl.Control"),
                QStringLiteral("AutoTuneCompleted"),
                m_daemon, SLOT(onAutoTuneCompleted(QString)));
}

void StatusMonitor::refreshAll()
{
    m_daemon->snapshot();
    m_daemon->runtimeState();
    m_daemon->controlStatus();
    m_daemon->draftConfig();
    m_daemon->degradedSummary();
}
