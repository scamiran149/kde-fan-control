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

// --- Read methods (async, emit results via signals) ---

void DaemonInterface::snapshot()
{
    callAsync(s_inventoryIface, QStringLiteral("snapshot"), {},
              [this](const QString &json) { Q_EMIT snapshotResult(json); });
}

void DaemonInterface::draftConfig()
{
    callAsync(s_lifecycleIface, QStringLiteral("get_draft_config"), {},
              [this](const QString &json) { Q_EMIT draftConfigResult(json); });
}

void DaemonInterface::appliedConfig()
{
    callAsync(s_lifecycleIface, QStringLiteral("get_applied_config"), {},
              [this](const QString &json) { Q_EMIT appliedConfigResult(json); });
}

void DaemonInterface::degradedSummary()
{
    callAsync(s_lifecycleIface, QStringLiteral("get_degraded_summary"), {},
              [this](const QString &json) { Q_EMIT degradedSummaryResult(json); });
}

void DaemonInterface::lifecycleEvents()
{
    callAsync(s_lifecycleIface, QStringLiteral("get_lifecycle_events"), {},
              [this](const QString &json) { Q_EMIT lifecycleEventsResult(json); });
}

void DaemonInterface::runtimeState()
{
    callAsync(s_lifecycleIface, QStringLiteral("get_runtime_state"), {},
              [this](const QString &json) { Q_EMIT runtimeStateResult(json); });
}

void DaemonInterface::controlStatus()
{
    callAsync(s_controlIface, QStringLiteral("get_control_status"), {},
              [this](const QString &json) { Q_EMIT controlStatusResult(json); });
}

void DaemonInterface::autoTuneResult(const QString &fanId)
{
    callAsync(s_controlIface, QStringLiteral("get_auto_tune_result"),
              {QVariant::fromValue(fanId)},
              [this, fanId](const QString &json) { Q_EMIT autoTuneResultReady(fanId, json); });
}

// --- Write methods (async, require root) ---

void DaemonInterface::setSensorName(const QString &id, const QString &name)
{
    callAsyncVoid(s_inventoryIface, QStringLiteral("set_sensor_name"),
                  {QVariant::fromValue(id), QVariant::fromValue(name)},
                  QStringLiteral("setSensorName"));
}

void DaemonInterface::setFanName(const QString &id, const QString &name)
{
    callAsyncVoid(s_inventoryIface, QStringLiteral("set_fan_name"),
                  {QVariant::fromValue(id), QVariant::fromValue(name)},
                  QStringLiteral("setFanName"));
}

void DaemonInterface::removeSensorName(const QString &id)
{
    callAsyncVoid(s_inventoryIface, QStringLiteral("remove_sensor_name"),
                  {QVariant::fromValue(id)},
                  QStringLiteral("removeSensorName"));
}

void DaemonInterface::removeFanName(const QString &id)
{
    callAsyncVoid(s_inventoryIface, QStringLiteral("remove_fan_name"),
                  {QVariant::fromValue(id)},
                  QStringLiteral("removeFanName"));
}

void DaemonInterface::setDraftFanEnrollment(const QString &fanId, const QString &draftJson)
{
    callAsyncVoid(s_lifecycleIface, QStringLiteral("set_draft_fan_enrollment"),
                  {QVariant::fromValue(fanId), QVariant::fromValue(draftJson)},
                  QStringLiteral("setDraftFanEnrollment"));
}

void DaemonInterface::removeDraftFan(const QString &fanId)
{
    callAsyncVoid(s_lifecycleIface, QStringLiteral("remove_draft_fan"),
                  {QVariant::fromValue(fanId)},
                  QStringLiteral("removeDraftFan"));
}

void DaemonInterface::discardDraft()
{
    callAsyncVoid(s_lifecycleIface, QStringLiteral("discard_draft"), {},
                  QStringLiteral("discardDraft"));
}

void DaemonInterface::validateDraft()
{
    callAsync(s_lifecycleIface, QStringLiteral("validate_draft"), {},
              [this](const QString &json) {
                  Q_EMIT writeSucceeded(QStringLiteral("validateDraft"));
              });
}

void DaemonInterface::applyDraft()
{
    callAsync(s_lifecycleIface, QStringLiteral("apply_draft"), {},
              [this](const QString &json) {
                  Q_EMIT writeSucceeded(QStringLiteral("applyDraft"));
              });
}

void DaemonInterface::startAutoTune(const QString &fanId)
{
    callAsyncVoid(s_controlIface, QStringLiteral("start_auto_tune"),
                  {QVariant::fromValue(fanId)},
                  QStringLiteral("startAutoTune"));
}

void DaemonInterface::acceptAutoTune(const QString &fanId)
{
    callAsync(s_controlIface, QStringLiteral("accept_auto_tune"),
              {QVariant::fromValue(fanId)},
              [this](const QString &json) {
                  Q_EMIT writeSucceeded(QStringLiteral("acceptAutoTune"));
              });
}

void DaemonInterface::setDraftFanControlProfile(const QString &fanId, const QString &profileJson)
{
    callAsyncVoid(s_controlIface, QStringLiteral("set_draft_fan_control_profile"),
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

    QDBusPendingCall asyncCall = iface.asyncCall(method, args);
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

    QDBusPendingCall asyncCall = iface.asyncCall(method, args);
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