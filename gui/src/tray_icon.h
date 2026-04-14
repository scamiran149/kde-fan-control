/*
 * KDE Fan Control — System Tray Icon
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * KStatusNotifierItem-based system tray icon that reflects
 * fan status severity, shows managed fan counts and alerts,
 * and provides context menu actions for the main window.
 */

#ifndef TRAY_ICON_H
#define TRAY_ICON_H

#include <QObject>
#include <QString>

#include <kstatusnotifieritem.h>
class FanListModel;
class OverviewModel;
class StatusMonitor;
class QWindow;

class TrayIcon : public QObject
{
    Q_OBJECT

    Q_PROPERTY(QString worstSeverity READ worstSeverity NOTIFY worstSeverityChanged)
    Q_PROPERTY(int managedFanCount READ managedFanCount NOTIFY managedFanCountChanged)
    Q_PROPERTY(int alertCount READ alertCount NOTIFY alertCountChanged)
    Q_PROPERTY(bool daemonConnected READ daemonConnected NOTIFY daemonConnectedChanged)
    Q_PROPERTY(bool hasStickyAlerts READ hasStickyAlerts NOTIFY hasStickyAlertsChanged)

public:
    explicit TrayIcon(StatusMonitor *statusMonitor,
                      OverviewModel *overviewModel,
                      QObject *parent = nullptr);

    QString worstSeverity() const { return m_worstSeverity; }
    int managedFanCount() const { return m_managedFanCount; }
    int alertCount() const { return m_alertCount; }
    bool daemonConnected() const { return m_daemonConnected; }
    bool hasStickyAlerts() const { return m_alertCount > 0; }

    void setDaemonConnected(bool connected);

    // Called by NotificationHandler when user acknowledges alerts
    Q_INVOKABLE void acknowledgeAlerts();

    // Associate the tray icon with a window so that primary-click
    // raises the window and Plasma can match the tray entry to the app.
    void setAssociatedWindow(QWindow *window);

signals:
    void worstSeverityChanged();
    void managedFanCountChanged();
    void alertCountChanged();
    void daemonConnectedChanged();
    void hasStickyAlertsChanged();
    void activateMainWindow();
    void showStatusPopover();

public slots:
    void updateSeverity();

private:
    void updateTrayIcon();
    void updateToolTip();

    // Severity precedence per UI-SPEC:
    // fallback > degraded > high-temp > managed > unmanaged > disconnected
    static int severityRank(const QString &state, bool highTempAlert);
    static QString severityIcon(const QString &severity);
    static KStatusNotifierItem::ItemStatus severityStatus(const QString &severity);

    KStatusNotifierItem *m_sni;
    StatusMonitor *m_statusMonitor;
    OverviewModel *m_overviewModel;

    QString m_worstSeverity;
    int m_managedFanCount = 0;
    int m_alertCount = 0;
    bool m_daemonConnected = false;
    bool m_alertsAcknowledged = false;
};

#endif // TRAY_ICON_H