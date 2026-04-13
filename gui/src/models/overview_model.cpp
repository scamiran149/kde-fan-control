/*
 * KDE Fan Control — Overview Model Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "overview_model.h"

#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QDebug>

OverviewModel::OverviewModel(QObject *parent)
    : QAbstractListModel(parent)
{
}

int OverviewModel::rowCount(const QModelIndex &parent) const
{
    if (parent.isValid())
        return 0;
    return m_rows.count();
}

QVariant OverviewModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_rows.count())
        return QVariant();

    OverviewFanRow *row = m_rows.at(index.row());

    switch (role) {
    case RowObjectRole:  return QVariant::fromValue(row);
    case FanIdRole:      return row->fanId();
    case DisplayNameRole: return row->displayName();
    case OrderingBucketRole: return row->orderingBucket();
    case VisualStateRole: return row->visualState();
    default:             return QVariant();
    }
}

QHash<int, QByteArray> OverviewModel::roleNames() const
{
    static const QHash<int, QByteArray> roles = {
        {RowObjectRole,     "rowObject"},
        {FanIdRole,         "fanId"},
        {DisplayNameRole,   "displayName"},
        {OrderingBucketRole, "orderingBucket"},
        {VisualStateRole,   "visualState"},
    };
    return roles;
}

OverviewFanRow *OverviewModel::rowByFanId(const QString &fanId) const
{
    return m_index.value(fanId, nullptr);
}

void OverviewModel::applyStructure(const QString &structureJson)
{
    QJsonParseError err;
    QJsonObject doc = QJsonDocument::fromJson(structureJson.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError && !structureJson.isEmpty()) {
        qWarning() << "OverviewModel::applyStructure: JSON parse error:" << err.errorString();
        return;
    }

    QJsonArray rows = doc.value(QStringLiteral("rows")).toArray();

    QStringList newFanIdOrder;
    QMap<QString, QJsonObject> newRowData;
    for (const QJsonValue &val : rows) {
        QJsonObject row = val.toObject();
        QString fanId = row.value(QStringLiteral("fan_id")).toString();
        newFanIdOrder.append(fanId);
        newRowData[fanId] = row;
    }

    QStringList currentFanIdOrder;
    for (const OverviewFanRow *row : m_rows) {
        currentFanIdOrder.append(row->fanId());
    }

    bool structureChanged = (currentFanIdOrder != newFanIdOrder);

    if (!m_initialized || structureChanged) {
        beginResetModel();
        qDeleteAll(m_rows);
        m_rows.clear();
        m_index.clear();

        for (const QString &fanId : newFanIdOrder) {
            auto *row = new OverviewFanRow(this);
            applyStructureToRow(row, newRowData[fanId]);
            m_rows.append(row);
            m_index[fanId] = row;
        }

        endResetModel();
        m_initialized = true;
        return;
    }

    for (int i = 0; i < m_rows.count(); ++i) {
        OverviewFanRow *row = m_rows.at(i);
        QJsonObject obj = newRowData.value(row->fanId());
        QString oldBucket = row->orderingBucket();
        applyStructureToRow(row, obj);
    }
}

void OverviewModel::applyTelemetry(const QString &telemetryJson)
{
    QJsonParseError err;
    QJsonObject doc = QJsonDocument::fromJson(telemetryJson.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError && !telemetryJson.isEmpty()) {
        qWarning() << "OverviewModel::applyTelemetry: JSON parse error:" << err.errorString();
        return;
    }

    QJsonArray rows = doc.value(QStringLiteral("rows")).toArray();
    for (const QJsonValue &val : rows) {
        QJsonObject obj = val.toObject();
        QString fanId = obj.value(QStringLiteral("fan_id")).toString();
        OverviewFanRow *row = m_index.value(fanId, nullptr);
        if (!row)
            continue;

        QString oldState = row->visualState();
        bool oldHighTemp = row->highTempAlert();

        applyTelemetryToRow(row, obj);

        QString newState = row->visualState();
        bool newHighTemp = row->highTempAlert();

        if (oldState != newState || oldHighTemp != newHighTemp) {
            int idx = m_rows.indexOf(row);
            if (idx >= 0) {
                QModelIndex modelIdx = index(idx, 0);
                QVector<int> roles;
                if (oldState != newState)
                    roles << VisualStateRole;
                emit dataChanged(modelIdx, modelIdx, roles);
            }
        }
    }
}

void OverviewModel::applyStructureToRow(OverviewFanRow *row, const QJsonObject &obj)
{
    row->setFanId(obj.value(QStringLiteral("fan_id")).toString());
    row->setDisplayName(obj.value(QStringLiteral("display_name")).toString());
    row->setFriendlyName(obj.value(QStringLiteral("friendly_name")).toString(QString()));
    row->setHardwareLabel(obj.value(QStringLiteral("hardware_label")).toString(QString()));
    row->setSupportState(obj.value(QStringLiteral("support_state")).toString());
    row->setControlMode(obj.value(QStringLiteral("control_mode")).toString(QString()));
    row->setHasTach(obj.value(QStringLiteral("has_tach")).toBool(false));
    row->setSupportReason(obj.value(QStringLiteral("support_reason")).toString(QString()));
    row->setOrderingBucket(obj.value(QStringLiteral("ordering_bucket")).toString());
    row->setStateText(obj.value(QStringLiteral("state_text")).toString());
    row->setStateIconName(obj.value(QStringLiteral("state_icon_name")).toString());
    row->setStateColor(obj.value(QStringLiteral("state_color")).toString());
    row->setShowSupportReason(obj.value(QStringLiteral("show_support_reason")).toBool(false));
}

void OverviewModel::applyTelemetryToRow(OverviewFanRow *row, const QJsonObject &obj)
{
    row->setTemperatureMillidegrees(obj.value(QStringLiteral("temperature_millidegrees")).toVariant().toLongLong());
    row->setTemperatureText(obj.value(QStringLiteral("temperature_text")).toString());
    row->setRpm(obj.value(QStringLiteral("rpm")).toInt(0));
    row->setRpmText(obj.value(QStringLiteral("rpm_text")).toString());
    row->setOutputPercent(obj.value(QStringLiteral("output_percent")).toDouble(0.0));
    row->setOutputText(obj.value(QStringLiteral("output_text")).toString());
    row->setOutputFillRatio(obj.value(QStringLiteral("output_fill_ratio")).toDouble(0.0));
    row->setHighTempAlert(obj.value(QStringLiteral("high_temp_alert")).toBool(false));
    row->setShowRpm(obj.value(QStringLiteral("show_rpm")).toBool(false));
    row->setShowOutput(obj.value(QStringLiteral("show_output")).toBool(false));
    row->setVisualState(obj.value(QStringLiteral("visual_state")).toString());
}