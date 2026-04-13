/*
 * KDE Fan Control — Notification Handler Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Emits desktop notifications only on TRANSITIONS into degraded,
 * fallback, and high-temp alert states per D-11.
 *
 * Reads from OverviewModel (structural path) rather than FanListModel
 * to decouple from 100ms telemetry churn. Only reacts to structural
 * model changes (dataChanged with VisualStateRole, modelReset,
 * rowsInserted/rowsRemoved).
 */

#include "notification_handler.h"
#include "status_monitor.h"
#include "models/overview_model.h"
#include "models/overview_fan_row.h"

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
                                         OverviewModel *overviewModel,
                                         QObject *parent)
    : QObject(parent)
    , m_statusMonitor(statusMonitor)
    , m_overviewModel(overviewModel)
{
    connect(m_overviewModel, &OverviewModel::modelReset,
            this, &NotificationHandler::onStructureChanged);
    connect(m_overviewModel, &OverviewModel::rowsInserted,
            this, &NotificationHandler::onStructureChanged);
    connect(m_overviewModel, &OverviewModel::rowsRemoved,
            this, &NotificationHandler::onStructureChanged);
    connect(m_overviewModel, &OverviewModel::dataChanged,
            this, &NotificationHandler::onStructureChanged);
}

void NotificationHandler::onStructureChanged()
{
    if (!m_initialized) {
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
        return;
    }

    QMap<QString, FanState> currentSnapshot;
    int fallbackCount = 0;
    int degradedCount = 0;
    int highTempCount = 0;

    const int rowCount = m_overviewModel->rowCount();
    for (int i = 0; i < rowCount; ++i) {
        QModelIndex idx = m_overviewModel->index(i, 0);
        OverviewFanRow *row = m_overviewModel->data(idx, OverviewModel::RowObjectRole).value<OverviewFanRow *>();
        if (!row)
            continue;

        QString fanId = row->fanId();
        QString state = row->visualState();
        bool highTemp = row->highTempAlert();

        FanState fs;
        fs.state = state;
        fs.hasHighTemp = highTemp;
        currentSnapshot[fanId] = fs;

        if (state == QStringLiteral("fallback")) fallbackCount++;
        if (state == QStringLiteral("degraded")) degradedCount++;
        if (highTemp && state == QStringLiteral("managed")) highTempCount++;
    }

    bool newFallback = false;
    bool newDegraded = false;
    bool newHighTemp = false;

    for (auto it = currentSnapshot.constBegin(); it != currentSnapshot.constEnd(); ++it) {
        const QString &fanId = it.key();
        const FanState &current = it.value();
        const FanState previous = m_previousState.value(fanId);

        if (current.state == QStringLiteral("fallback") &&
            previous.state != QStringLiteral("fallback")) {
            newFallback = true;
        }

        if (current.state == QStringLiteral("degraded") &&
            previous.state != QStringLiteral("degraded") &&
            previous.state != QStringLiteral("fallback")) {
            newDegraded = true;
        }

        if (current.hasHighTemp && !previous.hasHighTemp &&
            current.state == QStringLiteral("managed")) {
            newHighTemp = true;
        }
    }

    m_previousState = currentSnapshot;

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
}