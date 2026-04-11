/*
 * KDE Fan Control — Notification Handler
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Emits desktop notifications only on TRANSITIONS into important
 * alert states: degraded, fallback, and high-temperature alert.
 * Per D-11, notifications never fire on status updates that are
 * not transitions into these states.
 * Per D-12, alerts remain sticky in tray/main window until acknowledged.
 */

#ifndef NOTIFICATION_HANDLER_H
#define NOTIFICATION_HANDLER_H

#include <QObject>
#include <QMap>
#include <QString>

class StatusMonitor;
class FanListModel;

class NotificationHandler : public QObject
{
    Q_OBJECT

public:
    explicit NotificationHandler(StatusMonitor *statusMonitor,
                                 FanListModel *fanModel,
                                 QObject *parent = nullptr);

    // Called when the user acknowledges alerts from tray or main window
    Q_INVOKABLE void clearAcknowledgedState();

private slots:
    void onModelReset();
    void onDataChanged();

private:
    void checkTransitions();

    StatusMonitor *m_statusMonitor;
    FanListModel *m_fanModel;

    // Tracks previous state per fan to detect transitions.
    // Maps fan_id -> { state, hasHighTemp }
    struct FanState {
        QString state;
        bool hasHighTemp = false;
    };
    QMap<QString, FanState> m_previousState;

    bool m_initialized = false;
};

#endif // NOTIFICATION_HANDLER_H