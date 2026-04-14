/*
 * KDE Fan Control — System Tray Icon Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Creates and manages the KStatusNotifierItem-based system tray icon.
 * Severity precedence per UI-SPEC:
 *   fallback > degraded > high-temp > managed > unmanaged > disconnected
 *
 * Reads state from OverviewModel (structural path) rather than
 * FanListModel to decouple from 100ms telemetry churn.
 */

#include "tray_icon.h"
#include "status_monitor.h"
#include "models/overview_model.h"
#include "models/overview_fan_row.h"

#include <knotification.h>

#include <QMenu>
#include <QAction>
#include <QCoreApplication>
#include <QDebug>
#include <QWindow>

TrayIcon::TrayIcon(StatusMonitor *statusMonitor,
                   OverviewModel *overviewModel,
                   QObject *parent)
    : QObject(parent)
    , m_statusMonitor(statusMonitor)
    , m_overviewModel(overviewModel)
    , m_worstSeverity(QStringLiteral("disconnected"))
{
    m_sni = new KStatusNotifierItem(QStringLiteral("org.kde.fancontrol"), this);
    m_sni->setCategory(KStatusNotifierItem::SystemServices);
    m_sni->setTitle(QStringLiteral("Fan Control"));

    m_sni->setIconByName(QStringLiteral("network-offline-symbolic"));
    m_sni->setStatus(KStatusNotifierItem::Passive);
    m_sni->setToolTipIconByName(QStringLiteral("network-offline-symbolic"));
    m_sni->setToolTipTitle(QStringLiteral("Fan Control"));
    m_sni->setToolTipSubTitle(QStringLiteral("Daemon disconnected"));

    auto *openAction = new QAction(QStringLiteral("Open Fan Control"), this);
    connect(openAction, &QAction::triggered, this, &TrayIcon::activateMainWindow);

    auto *popoverAction = new QAction(QStringLiteral("Status Overview"), this);
    connect(popoverAction, &QAction::triggered, this, &TrayIcon::showStatusPopover);

    auto *ackAction = new QAction(QStringLiteral("Acknowledge Alerts"), this);
    connect(ackAction, &QAction::triggered, this, &TrayIcon::acknowledgeAlerts);

    auto *menu = new QMenu();
    menu->addAction(openAction);
    menu->addAction(popoverAction);
    menu->addSeparator();
    menu->addAction(ackAction);

    m_sni->setContextMenu(menu);

    connect(m_sni, &KStatusNotifierItem::activateRequested,
            this, &TrayIcon::activateMainWindow);

    connect(m_statusMonitor, &StatusMonitor::daemonConnectedChanged,
            this, [this]() {
                bool connected = m_statusMonitor->daemonConnected();
                setDaemonConnected(connected);
            });

    // Connect to structural model changes only (not telemetry)
    connect(m_overviewModel, &OverviewModel::modelReset,
            this, &TrayIcon::updateSeverity);
    connect(m_overviewModel, &OverviewModel::rowsInserted,
            this, &TrayIcon::updateSeverity);
    connect(m_overviewModel, &OverviewModel::rowsRemoved,
            this, &TrayIcon::updateSeverity);
    // dataChanged fires for structural roles (VisualStateRole etc)
    connect(m_overviewModel, &OverviewModel::dataChanged,
            this, &TrayIcon::updateSeverity);

    m_daemonConnected = m_statusMonitor->daemonConnected();
}

void TrayIcon::setDaemonConnected(bool connected)
{
    if (m_daemonConnected == connected)
        return;
    m_daemonConnected = connected;
    Q_EMIT daemonConnectedChanged();

    if (connected) {
        m_alertsAcknowledged = false;
        updateSeverity();
    } else {
        m_managedFanCount = 0;
        m_alertCount = 0;
        Q_EMIT managedFanCountChanged();
        Q_EMIT alertCountChanged();
        Q_EMIT hasStickyAlertsChanged();

        m_worstSeverity = QStringLiteral("disconnected");
        Q_EMIT worstSeverityChanged();
        updateTrayIcon();
        updateToolTip();
    }
}

void TrayIcon::updateSeverity()
{
    if (!m_daemonConnected) {
        return;
    }

    int managedCount = 0;
    int alertCount = 0;
    int worstRank = 999;

    const int rowCount = m_overviewModel->rowCount();
    for (int i = 0; i < rowCount; ++i) {
        QModelIndex idx = m_overviewModel->index(i, 0);
        OverviewFanRow *row = m_overviewModel->data(idx, OverviewModel::RowObjectRole).value<OverviewFanRow *>();
        if (!row)
            continue;

        QString state = row->visualState();
        bool highTemp = row->highTempAlert();

        int rank = severityRank(state, highTemp);
        if (rank < worstRank) {
            worstRank = rank;
        }

        if (state == QStringLiteral("managed")) {
            managedCount++;
        }
        if (state == QStringLiteral("degraded") ||
            state == QStringLiteral("fallback") ||
            highTemp) {
            alertCount++;
        }
    }

    QString newSeverity;
    if (worstRank <= 0)      newSeverity = QStringLiteral("fallback");
    else if (worstRank <= 1)  newSeverity = QStringLiteral("degraded");
    else if (worstRank <= 2)  newSeverity = QStringLiteral("high-temp");
    else if (worstRank <= 3)  newSeverity = QStringLiteral("managed");
    else if (worstRank <= 4)  newSeverity = QStringLiteral("unmanaged");
    else                      newSeverity = QStringLiteral("disconnected");

    bool severityChanged = (newSeverity != m_worstSeverity);
    bool countChanged = (managedCount != m_managedFanCount);
    bool alertChanged = (alertCount != m_alertCount);

    if (severityChanged) {
        m_worstSeverity = newSeverity;
        Q_EMIT worstSeverityChanged();
    }
    if (countChanged) {
        m_managedFanCount = managedCount;
        Q_EMIT managedFanCountChanged();
    }
    if (alertChanged) {
        m_alertCount = alertCount;
        Q_EMIT alertCountChanged();
        Q_EMIT hasStickyAlertsChanged();
        if (alertCount > 0) {
            m_alertsAcknowledged = false;
        }
    }

    if (severityChanged) {
        updateTrayIcon();
    }
    if (severityChanged || countChanged || alertChanged) {
        updateToolTip();
    }
}

void TrayIcon::updateTrayIcon()
{
    m_sni->setIconByName(severityIcon(m_worstSeverity));
    m_sni->setStatus(severityStatus(m_worstSeverity));
}

void TrayIcon::updateToolTip()
{
    QString title = QStringLiteral("Fan Control");
    QString subtitle;

    if (m_worstSeverity == QStringLiteral("disconnected")) {
        subtitle = QStringLiteral("Daemon disconnected");
    } else {
        if (m_worstSeverity == QStringLiteral("fallback")) {
            subtitle = QStringLiteral("%1 fan(s) in fallback").arg(m_alertCount);
        } else if (m_worstSeverity == QStringLiteral("degraded")) {
            subtitle = QStringLiteral("%1 fan(s) degraded").arg(m_alertCount);
        } else if (m_worstSeverity == QStringLiteral("high-temp")) {
            subtitle = QStringLiteral("High temperature alert");
        } else if (m_worstSeverity == QStringLiteral("managed")) {
            subtitle = QStringLiteral("All fans managed");
        } else {
            subtitle = QStringLiteral("No managed fans");
        }

        subtitle += QStringLiteral("\n%1 managed, %2 alerts")
                        .arg(m_managedFanCount)
                        .arg(m_alertCount);
    }

    m_sni->setToolTipIconByName(severityIcon(m_worstSeverity));
    m_sni->setToolTipTitle(title);
    m_sni->setToolTipSubTitle(subtitle);
}

void TrayIcon::acknowledgeAlerts()
{
    if (m_alertCount > 0) {
        m_alertsAcknowledged = true;
        Q_EMIT hasStickyAlertsChanged();
    }
}

int TrayIcon::severityRank(const QString &state, bool highTempAlert)
{
    if (state == QStringLiteral("fallback"))     return 0;
    if (state == QStringLiteral("degraded"))      return 1;
    if (state == QStringLiteral("managed") && highTempAlert) return 2;
    if (state == QStringLiteral("managed"))        return 3;
    if (state == QStringLiteral("unmanaged"))     return 4;
    if (state == QStringLiteral("partial"))        return 5;
    if (state == QStringLiteral("unavailable"))    return 6;
    return 7;
}

QString TrayIcon::severityIcon(const QString &severity)
{
    if (severity == QStringLiteral("fallback"))    return QStringLiteral("dialog-error-symbolic");
    if (severity == QStringLiteral("degraded"))    return QStringLiteral("data-warning-symbolic");
    if (severity == QStringLiteral("high-temp"))   return QStringLiteral("temperature-high-symbolic");
    if (severity == QStringLiteral("managed"))     return QStringLiteral("emblem-ok-symbolic");
    if (severity == QStringLiteral("unmanaged"))    return QStringLiteral("dialog-information-symbolic");
    return QStringLiteral("network-offline-symbolic");
}

KStatusNotifierItem::ItemStatus TrayIcon::severityStatus(const QString &severity)
{
    if (severity == QStringLiteral("fallback"))    return KStatusNotifierItem::NeedsAttention;
    if (severity == QStringLiteral("degraded"))    return KStatusNotifierItem::NeedsAttention;
    if (severity == QStringLiteral("high-temp"))   return KStatusNotifierItem::NeedsAttention;
    if (severity == QStringLiteral("managed"))     return KStatusNotifierItem::Active;
    return KStatusNotifierItem::Passive;
}

void TrayIcon::setAssociatedWindow(QWindow *window)
{
    if (m_sni && window) {
        m_sni->setAssociatedWindow(window);
    }
}