/*
 * KDE Fan Control — Notification Handler Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Emits desktop notifications only on TRANSITIONS into degraded,
 * fallback, and high-temp alert states per D-11.
 *
 * Alert stickiness is managed by TrayIcon (tray popover) and the
 * main window (OverviewPage banners). This handler only fires the
 * transient desktop notification on the transition event.
 */

#include "notification_handler.h"
#include "status_monitor.h"
#include "models/fan_list_model.h"

#include <knotification.h>

#include <QDebug>
#include <QStandardPaths>

namespace {
QString notificationComponentName()
{
    return QStringLiteral("kdefancontrol");
}

bool notificationConfigAvailable()
{
    return !QStandardPaths::locate(QStandardPaths::GenericDataLocation,
                                   QStringLiteral("knotifications6/%1.notifyrc").arg(notificationComponentName()))
                .isEmpty()
        || !QStandardPaths::locate(QStandardPaths::GenericDataLocation,
                                   QStringLiteral("knotifications5/%1.notifyrc").arg(notificationComponentName()))
                .isEmpty();
}
}

NotificationHandler::NotificationHandler(StatusMonitor *statusMonitor,
                                         FanListModel *fanModel,
                                         QObject *parent)
    : QObject(parent)
    , m_statusMonitor(statusMonitor)
    , m_fanModel(fanModel)
{
    // Connect to model changes to detect state transitions
    connect(m_fanModel, &FanListModel::dataChanged,
            this, &NotificationHandler::onDataChanged);
    connect(m_fanModel, &FanListModel::rowsInserted,
            this, [this]() { checkTransitions(); });
    connect(m_fanModel, &FanListModel::modelReset,
            this, &NotificationHandler::onModelReset);
}

void NotificationHandler::onModelReset()
{
    // After a model reset, we need to rebuild our tracking state
    // without generating transition notifications
    m_previousState.clear();
    m_initialized = false;
    checkTransitions();
    m_initialized = true;
}

void NotificationHandler::onDataChanged()
{
    if (!m_initialized) {
        // First population — build baseline without notifications
        m_previousState.clear();
        checkTransitions();
        m_initialized = true;
        return;
    }
    checkTransitions();
}

void NotificationHandler::checkTransitions()
{
    if (!m_statusMonitor->daemonConnected()) {
        // Don't fire notifications when daemon is disconnected
        return;
    }

    QMap<QString, FanState> currentSnapshot;
    int fallbackCount = 0;
    int degradedCount = 0;
    int highTempCount = 0;

    const int rowCount = m_fanModel->rowCount();
    for (int i = 0; i < rowCount; ++i) {
        QModelIndex idx = m_fanModel->index(i, 0);
        QString fanId = m_fanModel->data(idx, FanListModel::FanIdRole).toString();
        QString state = m_fanModel->data(idx, FanListModel::StateRole).toString();
        bool highTemp = m_fanModel->data(idx, FanListModel::HighTempAlertRole).toBool();

        FanState fs;
        fs.state = state;
        fs.hasHighTemp = highTemp;
        currentSnapshot[fanId] = fs;

        if (state == QStringLiteral("fallback")) fallbackCount++;
        if (state == QStringLiteral("degraded")) degradedCount++;
        if (highTemp && state == QStringLiteral("managed")) highTempCount++;
    }

    // Detect transitions by comparing to previous state
    bool newFallback = false;
    bool newDegraded = false;
    bool newHighTemp = false;

    for (auto it = currentSnapshot.constBegin(); it != currentSnapshot.constEnd(); ++it) {
        const QString &fanId = it.key();
        const FanState &current = it.value();
        const FanState previous = m_previousState.value(fanId);

        // Detect transition INTO fallback
        if (current.state == QStringLiteral("fallback") &&
            previous.state != QStringLiteral("fallback")) {
            newFallback = true;
        }

        // Detect transition INTO degraded
        if (current.state == QStringLiteral("degraded") &&
            previous.state != QStringLiteral("degraded") &&
            previous.state != QStringLiteral("fallback")) {
            newDegraded = true;
        }

        // Detect transition INTO high-temp alert
        if (current.hasHighTemp && !previous.hasHighTemp &&
            current.state == QStringLiteral("managed")) {
            newHighTemp = true;
        }
    }

    // Update previous state for next comparison
    m_previousState = currentSnapshot;

    // Fire notifications per D-11 (only on transitions)
    if (!notificationConfigAvailable()) {
        return;
    }

    if (newFallback) {
        KNotification *n = KNotification::event(
            QStringLiteral("fallback-active"),
            QStringLiteral("Fallback active"),
            QStringLiteral("Managed fans were driven to safe maximum output. Open Fan Control now."),
            QStringLiteral("dialog-error-symbolic"),
            KNotification::CloseOnTimeout,
            notificationComponentName());
        n->setUrgency(KNotification::HighUrgency);
        n->sendEvent();
    }

    if (newDegraded) {
        KNotification *n = KNotification::event(
            QStringLiteral("degraded-state"),
            QStringLiteral("Fan control degraded"),
            QStringLiteral("One or more managed fans could not be controlled safely. Open Fan Control to review the reason."),
            QStringLiteral("data-warning-symbolic"),
            KNotification::CloseOnTimeout,
            notificationComponentName());
        n->setUrgency(KNotification::HighUrgency);
        n->sendEvent();
    }

    if (newHighTemp) {
        KNotification *n = KNotification::event(
            QStringLiteral("high-temp-alert"),
            QStringLiteral("High temperature alert"),
            QStringLiteral("A managed fan is above its target temperature. Check runtime status and tuning."),
            QStringLiteral("temperature-high-symbolic"),
            KNotification::CloseOnTimeout,
            notificationComponentName());
        n->setUrgency(KNotification::NormalUrgency);
        n->sendEvent();
    }
}

void NotificationHandler::clearAcknowledgedState()
{
    // Acknowledged state clearing is handled by TrayIcon which
    // manages the sticky state in the UI. This method is a stub
    // for future coordination if needed.
}
