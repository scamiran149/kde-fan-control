/*
 * KDE Fan Control — Status Monitor Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Dual-path refresh scheduler:
 *   - Overview telemetry: 100ms fast cycle → OverviewModel::applyTelemetry()
 *   - Overview structure:  2000ms cooldown  → OverviewModel::applyStructure()
 *   - Legacy merge path:  250ms coalesced   → FanListModel/SensorListModel
 */

#include "status_monitor.h"
#include "daemon_interface.h"
#include "models/fan_list_model.h"
#include "models/sensor_list_model.h"
#include "models/overview_model.h"

#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDebug>

static constexpr const char *s_service = "org.kde.FanControl";

static constexpr int s_telemetryIntervalMs  = 100;
static constexpr int s_structureIntervalMs  = 2000;
static constexpr int s_legacyIntervalMs     = 250;

StatusMonitor::StatusMonitor(DaemonInterface *daemon,
                             FanListModel *fanModel,
                             SensorListModel *sensorModel,
                             OverviewModel *overviewModel,
                             QObject *parent)
    : QObject(parent)
    , m_daemon(daemon)
    , m_fanModel(fanModel)
    , m_sensorModel(sensorModel)
    , m_overviewModel(overviewModel)
{
    // --- Legacy path ---
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

    // --- Overview path ---
    connect(m_daemon, &DaemonInterface::overviewStructureResult,
            this, &StatusMonitor::onOverviewStructureResult);
    connect(m_daemon, &DaemonInterface::overviewTelemetryResult,
            this, &StatusMonitor::onOverviewTelemetryResult);

    // Auto-tune should trigger a structural refresh when completed
    connect(m_daemon, &DaemonInterface::autoTuneResultReady,
            this, [this](const QString &fanId, const QString &json) {
                Q_UNUSED(json);
                Q_UNUSED(fanId);
                QMetaObject::invokeMethod(this, [this]() {
                    // Legacy path refresh for detail pages
                    m_daemon->runtimeState();
                    // Overview: force structure refresh for auto-tune
                    forceStructureRefresh();
                }, Qt::QueuedConnection);
            });

    // Write mutations → force structural refresh
    connect(m_daemon, &DaemonInterface::writeSucceeded,
            this, [this](const QString &method) {
                Q_UNUSED(method);
                forceStructureRefresh();
            });

    connectDBusSignals();

    // --- Legacy merge timer (250ms) ---
    m_refreshTimer = new QTimer(this);
    m_refreshTimer->setInterval(s_legacyIntervalMs);
    m_refreshTimer->setTimerType(Qt::PreciseTimer);
    m_refreshTimer->setSingleShot(false);
    connect(m_refreshTimer, &QTimer::timeout, this, &StatusMonitor::refreshAll);

    // --- Overview telemetry timer (100ms) ---
    m_telemetryTimer = new QTimer(this);
    m_telemetryTimer->setInterval(s_telemetryIntervalMs);
    m_telemetryTimer->setTimerType(Qt::PreciseTimer);
    m_telemetryTimer->setSingleShot(false);
    connect(m_telemetryTimer, &QTimer::timeout,
            this, &StatusMonitor::refreshTelemetry);

    // --- Overview structure timer (2000ms) ---
    m_structureTimer = new QTimer(this);
    m_structureTimer->setInterval(s_structureIntervalMs);
    m_structureTimer->setTimerType(Qt::PreciseTimer);
    m_structureTimer->setSingleShot(false);
    connect(m_structureTimer, &QTimer::timeout,
            this, &StatusMonitor::refreshStructure);

    m_lastStructureTime.start();

    onDaemonConnectedChanged();
}

void StatusMonitor::checkDaemonConnected()
{
    bool present = QDBusConnection::systemBus().interface()->isServiceRegistered(s_service);
    m_daemon->setConnected(present);
    if (present) {
        refreshAll();
        refreshTelemetry();
        forceStructureRefresh();
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
        refreshTelemetry();
        forceStructureRefresh();
        if (m_pollingEnabled) {
            m_refreshTimer->start();
            m_telemetryTimer->start();
            m_structureTimer->start();
        }
    } else {
        m_refreshTimer->stop();
        m_telemetryTimer->stop();
        m_structureTimer->stop();
        m_fanModel->refresh(QString(), QString(), QString());
        m_sensorModel->refresh(QString());
        m_overviewModel->applyStructure(QString());
        m_overviewModel->applyTelemetry(QString());
        m_cachedSnapshot.clear();
        m_cachedRuntimeState.clear();
        m_cachedDraftConfig.clear();
        m_cachedControlStatus.clear();
        m_cachedOverviewTelemetry.clear();
        m_cachedOverviewStructure.clear();
        m_snapshotArrived = m_runtimeStateArrived = m_controlStatusArrived = m_draftConfigArrived = false;
        m_telemetryArrived = m_structureArrived = false;
        m_sensorModelDirty = m_fanModelDirty = false;
        m_structureForcePending = false;
    }
}

// --- Legacy path callbacks ---

void StatusMonitor::onSnapshotResult(const QString &json)
{
    m_cachedSnapshot = json;
    m_snapshotArrived = true;
    m_sensorModelDirty = true;
    if (m_snapshotArrived && m_runtimeStateArrived) {
        applyPendingUpdates();
    }
}

void StatusMonitor::onRuntimeStateResult(const QString &json)
{
    m_cachedRuntimeState = json;
    m_runtimeStateArrived = true;
    if (m_snapshotArrived && m_runtimeStateArrived) {
        m_fanModelDirty = true;
        applyPendingUpdates();
    }
}

void StatusMonitor::onControlStatusResult(const QString &json)
{
    m_cachedControlStatus = json;
    m_controlStatusArrived = true;
    m_fanModelDirty = true;
    if (m_snapshotArrived && m_runtimeStateArrived) {
        applyPendingUpdates();
    }
}

void StatusMonitor::onDraftConfigResult(const QString &json)
{
    m_cachedDraftConfig = json;
    m_draftConfigArrived = true;
    m_fanModelDirty = true;
    if (m_snapshotArrived && m_runtimeStateArrived) {
        applyPendingUpdates();
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
    m_telemetryTimer->stop();
    m_structureTimer->stop();
    m_daemonConnected = false;
    Q_EMIT daemonConnectedChanged();
    m_fanModel->refresh(QString(), QString(), QString());
    m_sensorModel->refresh(QString());
    m_overviewModel->applyStructure(QString());
    m_overviewModel->applyTelemetry(QString());
    m_cachedSnapshot.clear();
    m_cachedRuntimeState.clear();
    m_cachedDraftConfig.clear();
    m_cachedControlStatus.clear();
    m_cachedOverviewTelemetry.clear();
    m_cachedOverviewStructure.clear();
    m_snapshotArrived = m_runtimeStateArrived = m_controlStatusArrived = m_draftConfigArrived = false;
    m_telemetryArrived = m_structureArrived = false;
    m_sensorModelDirty = m_fanModelDirty = false;
    m_structureForcePending = false;
}

void StatusMonitor::setPollingEnabled(bool enabled)
{
    if (m_pollingEnabled == enabled)
        return;
    m_pollingEnabled = enabled;
    Q_EMIT pollingEnabledChanged();
    if (enabled && m_daemonConnected) {
        m_refreshTimer->start();
        m_telemetryTimer->start();
        m_structureTimer->start();
    } else {
        m_refreshTimer->stop();
        m_telemetryTimer->stop();
        m_structureTimer->stop();
    }
}

void StatusMonitor::forceStructureRefresh()
{
    m_structureForcePending = true;
    if (m_daemonConnected) {
        m_daemon->overviewStructure();
    }
}

// --- Overview path callbacks ---

void StatusMonitor::onOverviewStructureResult(const QString &json)
{
    m_cachedOverviewStructure = json;
    m_structureArrived = true;
    applyOverviewStructure(json);
}

void StatusMonitor::onOverviewTelemetryResult(const QString &json)
{
    m_cachedOverviewTelemetry = json;
    m_telemetryArrived = true;
    applyOverviewTelemetry(json);
}

// --- Timer-driven dispatch ---

void StatusMonitor::refreshAll()
{
    m_snapshotArrived = m_runtimeStateArrived = m_controlStatusArrived = m_draftConfigArrived = false;
    m_sensorModelDirty = m_fanModelDirty = false;
    m_daemon->snapshot();
    m_daemon->runtimeState();
    m_daemon->controlStatus();
    m_daemon->draftConfig();
    m_daemon->degradedSummary();
}

void StatusMonitor::refreshTelemetry()
{
    if (!m_daemonConnected)
        return;
    m_telemetryArrived = false;
    m_daemon->overviewTelemetry();
}

void StatusMonitor::refreshStructure()
{
    if (!m_daemonConnected)
        return;
    // Cooldown-gated: only dispatch if enough time has elapsed
    // unless a force was requested
    if (!m_structureForcePending && m_lastStructureTime.elapsed() < s_structureIntervalMs) {
        return;
    }
    m_structureArrived = false;
    m_structureForcePending = false;
    m_daemon->overviewStructure();
}

// --- Apply methods ---

void StatusMonitor::applyPendingUpdates()
{
    if (m_sensorModelDirty) {
        m_sensorModel->refresh(m_cachedSnapshot);
        m_sensorModelDirty = false;
    }
    if (m_fanModelDirty && m_snapshotArrived && m_runtimeStateArrived) {
        m_fanModel->refresh(m_cachedSnapshot, m_cachedRuntimeState,
                            m_cachedDraftConfig, m_cachedControlStatus);
        m_fanModelDirty = false;
    }
}

void StatusMonitor::applyOverviewTelemetry(const QString &json)
{
    if (json.isEmpty())
        return;
    m_overviewModel->applyTelemetry(json);
}

void StatusMonitor::applyOverviewStructure(const QString &json)
{
    m_lastStructureTime.restart();
    if (json.isEmpty())
        return;
    m_overviewModel->applyStructure(json);
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
