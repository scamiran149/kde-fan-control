/*
 * KDE Fan Control — DBus Proxy Interface
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Central C++ proxy that communicates with the fan-control daemon
 * over the system bus. All daemon methods return JSON strings which
 * the GUI parses into structured data via the model layer.
 *
 * Write methods require root (UID 0) per the daemon's authorization
 * policy — see org.kde.FanControl.conf.
 */

#ifndef DAEMON_INTERFACE_H
#define DAEMON_INTERFACE_H

#include <QObject>
#include <QString>
#include <QDBusInterface>
#include <QDBusPendingCallWatcher>
#include <QDBusConnection>

class DaemonInterface : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
    Q_PROPERTY(bool canWrite READ canWrite CONSTANT)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)

public:
    explicit DaemonInterface(QObject *parent = nullptr);

    bool connected() const { return m_connected; }
    bool canWrite() const;
    QString lastError() const { return m_lastError; }

    // --- Read methods (JSON string responses) ---

    Q_INVOKABLE void snapshot();
    Q_INVOKABLE void draftConfig();
    Q_INVOKABLE void appliedConfig();
    Q_INVOKABLE void degradedSummary();
    Q_INVOKABLE void lifecycleEvents();
    Q_INVOKABLE void runtimeState();
    Q_INVOKABLE void controlStatus();
    Q_INVOKABLE void autoTuneResult(const QString &fanId);
    Q_INVOKABLE void overviewStructure();
    Q_INVOKABLE void overviewTelemetry();

    // --- Write methods (privileged, require root) ---

    Q_INVOKABLE void setSensorName(const QString &id, const QString &name);
    Q_INVOKABLE void setFanName(const QString &id, const QString &name);
    Q_INVOKABLE void removeSensorName(const QString &id);
    Q_INVOKABLE void removeFanName(const QString &id);
    Q_INVOKABLE void setDraftFanEnrollment(const QString &fanId, bool managed,
                                            const QString &controlMode,
                                            const QStringList &tempSources);
    Q_INVOKABLE void removeDraftFan(const QString &fanId);
    Q_INVOKABLE void discardDraft();
    Q_INVOKABLE void validateDraft();
    Q_INVOKABLE void applyDraft();
    Q_INVOKABLE void startAutoTune(const QString &fanId);
    Q_INVOKABLE void acceptAutoTune(const QString &fanId);
    Q_INVOKABLE void setDraftFanControlProfile(const QString &fanId, const QString &profileJson);

    // Used by StatusMonitor to connect signals.
    QDBusConnection systemBus() const;

signals:
    void connectedChanged();
    void lastErrorChanged();

    // Emitted when async read results arrive.
    void snapshotResult(const QString &json);
    void draftConfigResult(const QString &json);
    void appliedConfigResult(const QString &json);
    void degradedSummaryResult(const QString &json);
    void lifecycleEventsResult(const QString &json);
    void runtimeStateResult(const QString &json);
    void controlStatusResult(const QString &json);
    void autoTuneResultReady(const QString &fanId, const QString &json);
    void overviewStructureResult(const QString &json);
    void overviewTelemetryResult(const QString &json);

    // Emitted when write operations complete.
    void writeSucceeded(const QString &method);
    void writeFailed(const QString &method, const QString &error);

public slots:
    void setConnected(bool connected);
    void setLastError(const QString &error);

private slots:
    void handleNameOwnerChanged(const QString &name, const QString &oldOwner, const QString &newOwner);
    void onDraftChanged();
    void onAppliedConfigChanged();
    void onDegradedStateChanged();
    void onLifecycleEventAppended(const QString &eventKind, const QString &detail);
    void onAutoTuneCompleted(const QString &fanId);

private:
    void callAsync(const QString &interface, const QString &method,
                   const QList<QVariant> &args,
                   const std::function<void(const QString &)> &onSuccess);
    void callAsyncVoid(const QString &interface, const QString &method,
                       const QList<QVariant> &args,
                       const QString &writeMethodLabel);

    void handleDBusError(const QString &context, const QDBusError &error);

    static constexpr const char *s_service = "org.kde.FanControl";
    static constexpr const char *s_path = "/org/kde/FanControl";
    static constexpr const char *s_inventoryIface = "org.kde.FanControl.Inventory";
    static constexpr const char *s_lifecyclePath = "/org/kde/FanControl/Lifecycle";
    static constexpr const char *s_lifecycleIface = "org.kde.FanControl.Lifecycle";
    static constexpr const char *s_controlPath = "/org/kde/FanControl/Control";
    static constexpr const char *s_controlIface = "org.kde.FanControl.Control";

    bool m_connected = false;
    QString m_lastError;

    QDBusInterface m_inventoryIface;
    QDBusInterface m_lifecycleIface;
    QDBusInterface m_controlIface;
};

#endif // DAEMON_INTERFACE_H
