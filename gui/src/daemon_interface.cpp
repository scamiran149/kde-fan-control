/*
 * KDE Fan Control — DBus Proxy Interface Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "daemon_interface.h"

#include <QDBusReply>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusConnectionInterface>
#include <QDebug>

#include <unistd.h>

DaemonInterface::DaemonInterface(QObject *parent)
    : QObject(parent)
    , m_inventoryIface(s_service, s_path, s_inventoryIface, QDBusConnection::systemBus())
    , m_lifecycleIface(s_service, s_lifecyclePath, s_lifecycleIface, QDBusConnection::systemBus())
    , m_controlIface(s_service, s_controlPath, s_controlIface, QDBusConnection::systemBus())
{
    // Detect whether the daemon is reachable on the system bus.
    // We track name owner changes to set m_connected accordingly.
    QDBusConnection::systemBus().connect(
        s_service,
        s_path,
        s_inventoryIface,
        QString(),
        this, SLOT(setConnected(bool)));

    // More reliable: watch the name owner on the system bus.
    QDBusConnection::systemBus().registerObject(
        QStringLiteral("/org/kde/FanControl/GUI"), this);

    // Try a ping to set initial state.
    bool servicePresent = QDBusConnection::systemBus().interface()->isServiceRegistered(s_service);
    setConnected(servicePresent);

    // Connect name-owner-changed signal to track daemon availability.
    QDBusConnection::systemBus().connect(
        QStringLiteral("org.freedesktop.DBus"),
        QStringLiteral("/org/freedesktop/DBus"),
        QStringLiteral("org.freedesktop.DBus"),
        QStringLiteral("NameOwnerChanged"),
        this,
        SLOT(handleNameOwnerChanged(QString,QString,QString)));
}

QDBusConnection DaemonInterface::systemBus() const
{
    return QDBusConnection::systemBus();
}

bool DaemonInterface::canWrite() const
{
    return ::geteuid() == 0;
}

void DaemonInterface::setConnected(bool connected)
{
    if (m_connected != connected) {
        m_connected = connected;
        Q_EMIT connectedChanged();
    }
}

void DaemonInterface::setLastError(const QString &error)
{
    if (m_lastError != error) {
        m_lastError = error;
        Q_EMIT lastErrorChanged();
    }
}

void DaemonInterface::handleNameOwnerChanged(const QString &name, const QString &oldOwner, const QString &newOwner)
{
    Q_UNUSED(oldOwner);
    if (name == QLatin1String(s_service)) {
        if (newOwner.isEmpty()) {
            setConnected(false);
        } else if (oldOwner.isEmpty()) {
            setConnected(true);
        } else {
            setConnected(false);
            setConnected(true);
        }
    }
}

void DaemonInterface::onDraftChanged() { draftConfig(); }
void DaemonInterface::onAppliedConfigChanged() { appliedConfig(); runtimeState(); }
void DaemonInterface::onDegradedStateChanged() { degradedSummary(); runtimeState(); }

void DaemonInterface::onLifecycleEventAppended(const QString &eventKind, const QString &detail)
{
    Q_UNUSED(eventKind);
    Q_UNUSED(detail);
    lifecycleEvents();
}

void DaemonInterface::onAutoTuneCompleted(const QString &fanId)
{
    autoTuneResult(fanId);
    runtimeState();
}

// --- Read methods (async, emit results via signals) ---

void DaemonInterface::snapshot()
{
    callAsync(s_inventoryIface, QStringLiteral("Snapshot"), {},
              [this](const QString &json) { Q_EMIT snapshotResult(json); });
}

void DaemonInterface::draftConfig()
{
    callAsync(s_lifecycleIface, QStringLiteral("GetDraftConfig"), {},
              [this](const QString &json) { Q_EMIT draftConfigResult(json); });
}

void DaemonInterface::appliedConfig()
{
    callAsync(s_lifecycleIface, QStringLiteral("GetAppliedConfig"), {},
              [this](const QString &json) { Q_EMIT appliedConfigResult(json); });
}

void DaemonInterface::degradedSummary()
{
    callAsync(s_lifecycleIface, QStringLiteral("GetDegradedSummary"), {},
              [this](const QString &json) { Q_EMIT degradedSummaryResult(json); });
}

void DaemonInterface::lifecycleEvents()
{
    callAsync(s_lifecycleIface, QStringLiteral("GetLifecycleEvents"), {},
              [this](const QString &json) { Q_EMIT lifecycleEventsResult(json); });
}

void DaemonInterface::runtimeState()
{
    callAsync(s_lifecycleIface, QStringLiteral("GetRuntimeState"), {},
              [this](const QString &json) { Q_EMIT runtimeStateResult(json); });
}

void DaemonInterface::controlStatus()
{
    callAsync(s_controlIface, QStringLiteral("GetControlStatus"), {},
              [this](const QString &json) { Q_EMIT controlStatusResult(json); });
}

void DaemonInterface::autoTuneResult(const QString &fanId)
{
    callAsync(s_controlIface, QStringLiteral("GetAutoTuneResult"),
              {QVariant::fromValue(fanId)},
              [this, fanId](const QString &json) { Q_EMIT autoTuneResultReady(fanId, json); });
}

// --- Write methods (async, require root) ---

void DaemonInterface::setSensorName(const QString &id, const QString &name)
{
    callAsyncVoid(s_inventoryIface, QStringLiteral("SetSensorName"),
                  {QVariant::fromValue(id), QVariant::fromValue(name)},
                  QStringLiteral("setSensorName"));
}

void DaemonInterface::setFanName(const QString &id, const QString &name)
{
    callAsyncVoid(s_inventoryIface, QStringLiteral("SetFanName"),
                  {QVariant::fromValue(id), QVariant::fromValue(name)},
                  QStringLiteral("setFanName"));
}

void DaemonInterface::removeSensorName(const QString &id)
{
    callAsyncVoid(s_inventoryIface, QStringLiteral("RemoveSensorName"),
                  {QVariant::fromValue(id)},
                  QStringLiteral("removeSensorName"));
}

void DaemonInterface::removeFanName(const QString &id)
{
    callAsyncVoid(s_inventoryIface, QStringLiteral("RemoveFanName"),
                  {QVariant::fromValue(id)},
                  QStringLiteral("removeFanName"));
}

void DaemonInterface::setDraftFanEnrollment(const QString &fanId, bool managed,
                                            const QString &controlMode,
                                            const QStringList &tempSources,
                                            const QString &aggregation)
{
    callAsyncVoid(s_lifecycleIface, QStringLiteral("SetDraftFanEnrollment"),
                  {QVariant::fromValue(fanId),
                   QVariant::fromValue(managed),
                   QVariant::fromValue(controlMode),
                   QVariant::fromValue(tempSources),
                   QVariant::fromValue(aggregation)},
                  QStringLiteral("setDraftFanEnrollment"));
}

void DaemonInterface::removeDraftFan(const QString &fanId)
{
    callAsyncVoid(s_lifecycleIface, QStringLiteral("RemoveDraftFan"),
                  {QVariant::fromValue(fanId)},
                  QStringLiteral("removeDraftFan"));
}

void DaemonInterface::discardDraft()
{
    callAsyncVoid(s_lifecycleIface, QStringLiteral("DiscardDraft"), {},
                  QStringLiteral("discardDraft"));
}

void DaemonInterface::validateDraft()
{
    callAsync(s_lifecycleIface, QStringLiteral("ValidateDraft"), {},
              [this](const QString &json) {
                  Q_EMIT writeSucceeded(QStringLiteral("validateDraft"));
              });
}

void DaemonInterface::applyDraft()
{
    callAsync(s_lifecycleIface, QStringLiteral("ApplyDraft"), {},
              [this](const QString &json) {
                  Q_EMIT writeSucceeded(QStringLiteral("applyDraft"));
              });
}

void DaemonInterface::startAutoTune(const QString &fanId)
{
    callAsyncVoid(s_controlIface, QStringLiteral("StartAutoTune"),
                  {QVariant::fromValue(fanId)},
                  QStringLiteral("startAutoTune"));
}

void DaemonInterface::acceptAutoTune(const QString &fanId)
{
    callAsync(s_controlIface, QStringLiteral("AcceptAutoTune"),
              {QVariant::fromValue(fanId)},
              [this](const QString &json) {
                  Q_EMIT writeSucceeded(QStringLiteral("acceptAutoTune"));
              });
}

void DaemonInterface::setDraftFanControlProfile(const QString &fanId, const QString &profileJson)
{
    callAsyncVoid(s_controlIface, QStringLiteral("SetDraftFanControlProfile"),
                  {QVariant::fromValue(fanId), QVariant::fromValue(profileJson)},
                  QStringLiteral("setDraftFanControlProfile"));
}

// --- Private helpers ---

void DaemonInterface::callAsync(
    const QString &interface,
    const QString &method,
    const QList<QVariant> &args,
    const std::function<void(const QString &)> &onSuccess)
{
    QDBusInterface &iface = (interface == s_inventoryIface) ? m_inventoryIface
                         : (interface == s_lifecycleIface) ? m_lifecycleIface
                         : m_controlIface;

    QDBusPendingCall asyncCall = iface.asyncCallWithArgumentList(method, args);
    auto *watcher = new QDBusPendingCallWatcher(asyncCall, this);

    QObject::connect(watcher, &QDBusPendingCallWatcher::finished, this,
                     [this, onSuccess, method](QDBusPendingCallWatcher *w) {
                         w->deleteLater();
                         QDBusPendingReply<QString> reply = *w;
                         if (reply.isError()) {
                             handleDBusError(method, reply.error());
                         } else {
                             setLastError(QString());
                             onSuccess(reply.value());
                         }
                     });
}

void DaemonInterface::callAsyncVoid(
    const QString &interface,
    const QString &method,
    const QList<QVariant> &args,
    const QString &writeMethodLabel)
{
    QDBusInterface &iface = (interface == s_inventoryIface) ? m_inventoryIface
                         : (interface == s_lifecycleIface) ? m_lifecycleIface
                         : m_controlIface;

    QDBusPendingCall asyncCall = iface.asyncCallWithArgumentList(method, args);
    auto *watcher = new QDBusPendingCallWatcher(asyncCall, this);

    QObject::connect(watcher, &QDBusPendingCallWatcher::finished, this,
                     [this, writeMethodLabel](QDBusPendingCallWatcher *w) {
                         w->deleteLater();
                         QDBusPendingReply<void> reply = *w;
                         if (reply.isError()) {
                             handleDBusError(writeMethodLabel, reply.error());
                             Q_EMIT writeFailed(writeMethodLabel, reply.error().message());
                         } else {
                             setLastError(QString());
                             Q_EMIT writeSucceeded(writeMethodLabel);
                         }
                     });
}

void DaemonInterface::handleDBusError(const QString &context, const QDBusError &error)
{
    const QString msg = error.name() == QStringLiteral("org.freedesktop.DBus.Error.ServiceUnknown")
        ? QStringLiteral("Couldn't talk to the fan-control daemon. Check that the system service is running, then retry.")
        : error.message().isEmpty()
            ? QStringLiteral("DBus error in %1").arg(context)
            : QStringLiteral("%1: %2").arg(context, error.message());

    qWarning() << "DBus error in" << context << ":" << error.name() << error.message();
    setLastError(msg);

    if (error.name() == QStringLiteral("org.freedesktop.DBus.Error.ServiceUnknown")
        || error.name() == QStringLiteral("org.freedesktop.DBus.Error.NoReply")) {
        setConnected(false);
    }
}
