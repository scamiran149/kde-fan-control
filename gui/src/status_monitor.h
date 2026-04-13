/*
 * KDE Fan Control — Status Monitor
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Dual-path refresh scheduler:
 *
 *   Overview path (fast telemetry + rare structural):
 *     - Telemetry: 100ms polling cycle, updates OverviewModel in-place
 *     - Structure: 2000ms cooldown, rebuilds row membership/order/display
 *     - Structural refreshes are "forced" (bypass cooldown) on startup,
 *       daemon reconnect, and write mutations; "deferred" (cooldown-gated)
 *       for ordering/state/capability changes.
 *
 *   Legacy path (detail pages, inventory, sensors):
 *     - 250ms coalesced merge into FanListModel/SensorListModel
 *     - Keeps old 5-call merge logic for pages that still need it.
 */

#ifndef STATUS_MONITOR_H
#define STATUS_MONITOR_H

#include <QObject>
#include <QDBusConnection>
#include <QTimer>
#include <QElapsedTimer>

class DaemonInterface;
class FanListModel;
class SensorListModel;
class OverviewModel;

class StatusMonitor : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool daemonConnected READ daemonConnected NOTIFY daemonConnectedChanged)
    Q_PROPERTY(bool pollingEnabled READ pollingEnabled WRITE setPollingEnabled NOTIFY pollingEnabledChanged)

public:
    explicit StatusMonitor(DaemonInterface *daemon,
                            FanListModel *fanModel,
                            SensorListModel *sensorModel,
                            OverviewModel *overviewModel,
                            QObject *parent = nullptr);

    bool daemonConnected() const { return m_daemonConnected; }
    bool pollingEnabled() const { return m_pollingEnabled; }
    void setPollingEnabled(bool enabled);

    // Force an immediate structural refresh (bypasses cooldown).
    // Call from QML on visibility events, page transitions, etc.
    Q_INVOKABLE void forceStructureRefresh();

public slots:
    void checkDaemonConnected();

signals:
    void daemonConnectedChanged();
    void pollingEnabledChanged();

private slots:
    void onDaemonConnectedChanged();
    void onSnapshotResult(const QString &json);
    void onRuntimeStateResult(const QString &json);
    void onControlStatusResult(const QString &json);
    void onDraftConfigResult(const QString &json);
    void onDegradedSummaryResult(const QString &json);
    void onOverviewStructureResult(const QString &json);
    void onOverviewTelemetryResult(const QString &json);
    void onDaemonDisconnected();

private:
    void connectDBusSignals();
    void refreshAll();
    void applyPendingUpdates();

    // Overview path
    void refreshTelemetry();
    void refreshStructure();
    void applyOverviewTelemetry(const QString &json);
    void applyOverviewStructure(const QString &json);

    DaemonInterface *m_daemon;
    FanListModel *m_fanModel;
    SensorListModel *m_sensorModel;
    OverviewModel *m_overviewModel;
    bool m_daemonConnected = false;
    bool m_pollingEnabled = true;

    // Legacy path: 250ms coalesced merge timer
    QTimer *m_refreshTimer = nullptr;
    QString m_cachedSnapshot;
    QString m_cachedRuntimeState;
    QString m_cachedDraftConfig;
    QString m_cachedControlStatus;
    bool m_snapshotArrived = false;
    bool m_runtimeStateArrived = false;
    bool m_controlStatusArrived = false;
    bool m_draftConfigArrived = false;
    bool m_sensorModelDirty = false;
    bool m_fanModelDirty = false;

    // Overview telemetry path: 100ms fast polling
    QTimer *m_telemetryTimer = nullptr;
    QString m_cachedOverviewTelemetry;
    bool m_telemetryArrived = false;

    // Overview structural path: 2000ms cooldown
    QTimer *m_structureTimer = nullptr;
    QString m_cachedOverviewStructure;
    bool m_structureArrived = false;
    bool m_structureForcePending = false;
    QElapsedTimer m_lastStructureTime;
};

#endif // STATUS_MONITOR_H