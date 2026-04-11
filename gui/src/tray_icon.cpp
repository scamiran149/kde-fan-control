/*
 * KDE Fan Control — System Tray Icon Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Creates and manages the KStatusNotifierItem-based system tray icon.
 * Severity precedence per UI-SPEC:
 *   fallback > degraded > high-temp > managed > unmanaged > disconnected
 */

#include "tray_icon.h"
#include "status_monitor.h"
#include "models/fan_list_model.h"

#include <KNotifications/KNotification>

#include <QMenu>
#include <QAction>
#include <QCoreApplication>
#include <QDebug>

TrayIcon::TrayIcon(StatusMonitor *statusMonitor,
                   FanListModel *fanModel,
                   QObject *parent)
    : QObject(parent)
    , m_statusMonitor(statusMonitor)
    , m_fanModel(fanModel)
    , m_worstSeverity(QStringLiteral("disconnected"))
{
    // Create the system tray icon
    m_sni = new KStatusNotifierItem(QStringLiteral("org.kde.fancontrol"), this);
    m_sni->setCategory(KStatusNotifierItem::SystemServices);
    m_sni->setTitle(QStringLiteral("Fan Control"));

    // Set initial disconnected state
    m_sni->setIconByName(QStringLiteral("network-offline-symbolic"));
    m_sni->setStatus(KStatusNotifierItem::Passive);
    m_sni->setToolTipIconByName(QStringLiteral("network-offline-symbolic"));
    m_sni->setToolTipTitle(QStringLiteral("Fan Control"));
    m_sni->setToolTipSubTitle(QStringLiteral("Daemon disconnected"));

    // Context menu actions
    auto *openAction = new QAction(QStringLiteral("Open Fan Control"), this);
    connect(openAction, &QAction::triggered, this, []() {
        // Emit to QML/main window to activate/show
        // The main window visibility is managed by the QML layer
    });

    auto *ackAction = new QAction(QStringLiteral("Acknowledge alerts"), this);
    connect(ackAction, &QAction::triggered, this, &TrayIcon::acknowledgeAlerts);

    auto *quitAction = new QAction(QStringLiteral("Quit"), this);
    connect(quitAction, &QAction::triggered, &QCoreApplication::quit);

    auto *menu = new QMenu();
    menu->addAction(openAction);
    menu->addSeparator();
    menu->addAction(ackAction);
    menu->addSeparator();
    menu->addAction(quitAction);

    m_sni->setContextMenu(menu);

    // Connect to StatusMonitor for daemon state changes
    connect(m_statusMonitor, &StatusMonitor::daemonConnectedChanged,
            this, [this]() {
                bool connected = m_statusMonitor->daemonConnected();
                setDaemonConnected(connected);
            });

    // Connect to model changes to recompute severity
    connect(m_fanModel, &FanListModel::dataChanged,
            this, &TrayIcon::updateSeverity);
    connect(m_fanModel, &FanListModel::rowsInserted,
            this, &TrayIcon::updateSeverity);
    connect(m_fanModel, &FanListModel::rowsRemoved,
            this, &TrayIcon::updateSeverity);
    connect(m_fanModel, &FanListModel::modelReset,
            this, &TrayIcon::updateSeverity);

    // Initial state
    m_daemonConnected = m_statusMonitor->daemonConnected();
}

void TrayIcon::setDaemonConnected(bool connected)
{
    if (m_daemonConnected == connected)
        return;
    m_daemonConnected = connected;
    Q_EMIT daemonConnectedChanged();

    // Clear acknowledged state on reconnect since state may have changed
    if (connected) {
        m_alertsAcknowledged = false;
        updateSeverity();
    } else {
        // When daemon disconnects, reset to disconnected state
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
        // Already handled in setDaemonConnected
        return;
    }

    int managedCount = 0;
    int alertCount = 0;
    int worstRank = 999; // Lower is worse per severity precedence

    const int rowCount = m_fanModel->rowCount();
    for (int i = 0; i < rowCount; ++i) {
        QModelIndex idx = m_fanModel->index(i, 0);
        QString state = m_fanModel->data(idx, FanListModel::StateRole).toString();
        bool highTemp = m_fanModel->data(idx, FanListModel::HighTempAlertRole).toBool();

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

    // If no fans at all, severity is disconnected (handled above) or passive
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
        // New alerts clear the acknowledged flag per D-12
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
        // Primary: severity summary
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

        // Secondary: counts
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
        // Acknowledged alerts clear UI stickiness per D-12
        // This doesn't alter daemon state — it only clears the
        // sticky flag in the tray popover and main window banners.
        Q_EMIT hasStickyAlertsChanged();
    }
}

int TrayIcon::severityRank(const QString &state, bool highTempAlert)
{
    // Per UI-SPEC severity precedence:
    // fallback(0) > degraded(1) > high-temp(2) > managed(3) > unmanaged(4) > disconnected
    if (state == QStringLiteral("fallback"))     return 0;
    if (state == QStringLiteral("degraded"))      return 1;
    if (state == QStringLiteral("managed") && highTempAlert) return 2;
    if (state == QStringLiteral("managed"))        return 3;
    if (state == QStringLiteral("unmanaged"))     return 4;
    if (state == QStringLiteral("partial"))        return 5;
    if (state == QStringLiteral("unavailable"))    return 6;
    return 7; // unknown
}

QString TrayIcon::severityIcon(const QString &severity)
{
    if (severity == QStringLiteral("fallback"))    return QStringLiteral("dialog-error-symbolic");
    if (severity == QStringLiteral("degraded"))    return QStringLiteral("data-warning-symbolic");
    if (severity == QStringLiteral("high-temp"))   return QStringLiteral("temperature-high-symbolic");
    if (severity == QStringLiteral("managed"))     return QStringLiteral("emblem-ok-symbolic");
    if (severity == QStringLiteral("unmanaged"))    return QStringLiteral("dialog-information-symbolic");
    // disconnected or unknown
    return QStringLiteral("network-offline-symbolic");
}

KStatusNotifierItem::ItemStatus TrayIcon::severityStatus(const QString &severity)
{
    if (severity == QStringLiteral("fallback"))    return KStatusNotifierItem::NeedsAttention;
    if (severity == QStringLiteral("degraded"))    return KStatusNotifierItem::NeedsAttention;
    if (severity == QStringLiteral("high-temp"))   return KStatusNotifierItem::NeedsAttention;
    if (severity == QStringLiteral("managed"))     return KStatusNotifierItem::Active;
    // unmanaged or disconnected -> Passive
    return KStatusNotifierItem::Passive;
}