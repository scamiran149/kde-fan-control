/*
 * KDE Fan Control — Status Monitor
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Subscribes to DBus signals from the daemon and triggers reactive
 * model updates. All signal callbacks are marshaled to the main thread
 * via QMetaObject::invokeMethod with QueuedConnection.
 */

#ifndef STATUS_MONITOR_H
#define STATUS_MONITOR_H

#include <QObject>
#include <QDBusConnection>
#include <QTimer>

class DaemonInterface;
class FanListModel;
class SensorListModel;

class StatusMonitor : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool daemonConnected READ daemonConnected NOTIFY daemonConnectedChanged)
    Q_PROPERTY(bool pollingEnabled READ pollingEnabled WRITE setPollingEnabled NOTIFY pollingEnabledChanged)

public:
    explicit StatusMonitor(DaemonInterface *daemon,
                            FanListModel *fanModel,
                            SensorListModel *sensorModel,
                            QObject *parent = nullptr);

    bool daemonConnected() const { return m_daemonConnected; }
    bool pollingEnabled() const { return m_pollingEnabled; }
    void setPollingEnabled(bool enabled);

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
    void onDaemonDisconnected();

private:
    void connectDBusSignals();
    void refreshAll();

    DaemonInterface *m_daemon;
    FanListModel *m_fanModel;
    SensorListModel *m_sensorModel;
    bool m_daemonConnected = false;
    bool m_pollingEnabled = true;
    QTimer *m_refreshTimer = nullptr;

    // Cached JSON responses for merging into models.
    QString m_cachedSnapshot;
    QString m_cachedRuntimeState;
    QString m_cachedDraftConfig;
    QString m_cachedControlStatus;
};

#endif // STATUS_MONITOR_H
