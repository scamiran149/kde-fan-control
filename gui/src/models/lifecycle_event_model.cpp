/*
 * KDE Fan Control — Lifecycle Event Model Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "lifecycle_event_model.h"

#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QDebug>
#include <algorithm>

LifecycleEventModel::LifecycleEventModel(QObject *parent)
    : QAbstractListModel(parent)
{
}

int LifecycleEventModel::rowCount(const QModelIndex &parent) const
{
    if (parent.isValid())
        return 0;
    return m_events.count();
}

QVariant LifecycleEventModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_events.count())
        return QVariant();

    const LifecycleEventEntry &event = m_events.at(index.row());

    switch (role) {
    case TimestampRole: return event.timestamp;
    case EventTypeRole: return event.eventType;
    case ReasonRole:   return event.reason;
    case DetailRole:    return event.detail;
    case FanIdRole:     return event.fanId;
    default:            return QVariant();
    }
}

QHash<int, QByteArray> LifecycleEventModel::roleNames() const
{
    static const QHash<int, QByteArray> roles = {
        {TimestampRole, "timestamp"},
        {EventTypeRole, "eventType"},
        {ReasonRole,    "reason"},
        {DetailRole,    "detail"},
        {FanIdRole,     "fanId"}
    };
    return roles;
}

void LifecycleEventModel::refresh(const QString &eventsJson)
{
    QJsonParseError err;
    QJsonDocument doc = QJsonDocument::fromJson(eventsJson.toUtf8(), &err);
    if (err.error != QJsonParseError::NoError && !eventsJson.isEmpty()) {
        qWarning() << "LifecycleEventModel: JSON parse error:" << err.errorString();
        return;
    }

    QJsonArray eventsArray = doc.array();

    beginResetModel();
    m_events.clear();

    for (const QJsonValue &eventVal : eventsArray) {
        QJsonObject eventObj = eventVal.toObject();
        LifecycleEventEntry entry;

        entry.timestamp = eventObj.value(QStringLiteral("timestamp")).toString();
        entry.reason = eventObj.value(QStringLiteral("reason")).toString();
        entry.detail = eventObj.value(QStringLiteral("detail")).toString();
        entry.fanId = eventObj.value(QStringLiteral("fan_id")).toString();

        // Extract event_type from the reason structure.
        // The daemon sends structured degraded reasons.
        // For display, use the reason's Display representation.
        // The "event_type" may be embedded as a structured object.
        QJsonObject reasonObj = eventObj.value(QStringLiteral("reason")).toObject();
        if (!reasonObj.isEmpty()) {
            // Structured reason — extract kind
            entry.eventType = reasonObj.value(QStringLiteral("kind")).toString(
                eventObj.value(QStringLiteral("event_type")).toString());
            // Build human-readable reason from the kind field
            QString kind = entry.eventType;
            if (kind == QStringLiteral("fan_missing")) {
                entry.reason = QStringLiteral("Fan missing from hardware");
            } else if (kind == QStringLiteral("fan_no_longer_enrollable")) {
                entry.reason = QStringLiteral("Fan no longer enrollable");
            } else if (kind == QStringLiteral("control_mode_unavailable")) {
                entry.reason = QStringLiteral("Control mode no longer supported");
            } else if (kind == QStringLiteral("temp_source_missing")) {
                entry.reason = QStringLiteral("Temperature source missing");
            } else if (kind == QStringLiteral("fallback_active")) {
                entry.reason = QStringLiteral("Fallback mode active");
            } else if (kind == QStringLiteral("boot_restored")) {
                entry.reason = QStringLiteral("Fan restored on boot");
            } else if (kind == QStringLiteral("boot_reconciled")) {
                entry.reason = QStringLiteral("Boot reconciliation completed");
            } else if (kind == QStringLiteral("partial_boot_recovery")) {
                entry.reason = QStringLiteral("Partial boot recovery");
            } else {
                entry.reason = kind;
            }

            // Try to extract fan_id from reason structure if available
            if (entry.fanId.isEmpty()) {
                entry.fanId = reasonObj.value(QStringLiteral("fan_id")).toString();
            }
        } else {
            // Reason is a plain string
            entry.eventType = eventObj.value(QStringLiteral("event_type")).toString();
        }

        m_events.append(entry);
    }

    // Sort by timestamp descending (most recent first)
    std::sort(m_events.begin(), m_events.end(),
              [](const LifecycleEventEntry &a, const LifecycleEventEntry &b) {
                  return a.timestamp > b.timestamp;
              });

    endResetModel();
}