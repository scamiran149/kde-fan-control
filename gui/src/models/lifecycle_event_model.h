/*
 * KDE Fan Control — Lifecycle Event Model
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * QAbstractListModel for lifecycle event history.
 * Parses get_lifecycle_events() JSON from the daemon and presents
 * events sorted by timestamp descending (most recent first).
 */

#ifndef LIFECYCLE_EVENT_MODEL_H
#define LIFECYCLE_EVENT_MODEL_H

#include <QAbstractListModel>
#include <QList>
#include <QString>
#include <QDateTime>

struct LifecycleEventEntry {
    QString timestamp;
    QString eventType;
    QString reason;
    QString detail;
    QString fanId;
};

class LifecycleEventModel : public QAbstractListModel
{
    Q_OBJECT

public:
    enum Roles {
        TimestampRole = Qt::UserRole + 1,
        EventTypeRole,
        ReasonRole,
        DetailRole,
        FanIdRole
    };

    explicit LifecycleEventModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = QModelIndex()) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    Q_INVOKABLE void refresh(const QString &eventsJson);

private:
    QList<LifecycleEventEntry> m_events;
};

#endif // LIFECYCLE_EVENT_MODEL_H