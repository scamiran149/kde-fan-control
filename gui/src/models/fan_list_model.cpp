/*
 * KDE Fan Control — Fan List Model Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "fan_list_model.h"

#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QDebug>
#include <algorithm>

FanListModel::FanListModel(QObject *parent)
    : QAbstractListModel(parent)
{
}

int FanListModel::rowCount(const QModelIndex &parent) const
{
    if (parent.isValid())
        return 0;
    return m_fans.count();
}

QVariant FanListModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_fans.count())
        return QVariant();

    FanStateInfo *fan = m_fans.at(index.row());

    switch (role) {
    case FanIdRole:      return fan->fanId();
    case DisplayNameRole: return fan->displayName();
    case SupportStateRole: return fan->supportState();
    case ControlModeRole: return fan->controlMode();
    case StateRole:       return fan->state();
    case TemperatureMillidegRole: return fan->temperatureMillidegrees();
    case RpmRole:          return fan->rpm();
    case OutputPercentRole: return fan->outputPercent();
    case HasTachRole:      return fan->hasTach();
    case SupportReasonRole: return fan->supportReason();
    case HighTempAlertRole: return fan->highTempAlert();
    case SeverityOrderRole: return severityOrder(fan->state(), fan->highTempAlert());
    default:               return QVariant();
    }
}

QHash<int, QByteArray> FanListModel::roleNames() const
{
    static const QHash<int, QByteArray> roles = {
        {FanIdRole,           "fanId"},
        {DisplayNameRole,      "displayName"},
        {SupportStateRole,    "supportState"},
        {ControlModeRole,     "controlMode"},
        {StateRole,           "state"},
        {TemperatureMillidegRole, "temperatureMillidegrees"},
        {RpmRole,             "rpm"},
        {OutputPercentRole,   "outputPercent"},
        {HasTachRole,         "hasTach"},
        {SupportReasonRole,   "supportReason"},
        {HighTempAlertRole,   "highTempAlert"},
        {SeverityOrderRole,   "severityOrder"}
    };
    return roles;
}

void FanListModel::refresh(const QString &inventoryJson,
                            const QString &runtimeJson,
                            const QString &configJson)
{
    // Parse all three JSON inputs and merge into model rows.
    QJsonParseError err;
    QJsonObject inventory = QJsonDocument::fromJson(inventoryJson.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError && !inventoryJson.isEmpty()) {
        qWarning() << "FanListModel: inventory JSON parse error:" << err.errorString();
        return;
    }
    QJsonObject runtime = QJsonDocument::fromJson(runtimeJson.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError && !runtimeJson.isEmpty()) {
        qWarning() << "FanListModel: runtime JSON parse error:" << err.errorString();
        return;
    }
    QJsonObject config = QJsonDocument::fromJson(configJson.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError && !configJson.isEmpty()) {
        qWarning() << "FanListModel: config JSON parse error:" << err.errorString();
        return;
    }

    // Build a map from fan_id -> runtime status
    QMap<QString, QJsonObject> runtimeMap;
    QJsonObject fanStatuses = runtime.value(QStringLiteral("fan_statuses")).toObject();
    for (auto it = fanStatuses.begin(); it != fanStatuses.end(); ++it) {
        runtimeMap[it.key()] = it.value().toObject();
    }

    // Build a map from fan_id -> friendly name
    QMap<QString, QString> fanNames;
    QJsonObject fans = config.value(QStringLiteral("fans")).toObject();
    // Fans object is inside "draft" and "applied" top-level
    // For now, try to find friendly_names at top level
    QJsonObject friendlyNames = config.contains(QStringLiteral("friendly_names"))
        ? config.value(QStringLiteral("friendly_names")).toObject()
        : QJsonObject();

    QJsonObject fanNamesObj = friendlyNames.value(QStringLiteral("fans")).toObject();
    for (auto it = fanNamesObj.begin(); it != fanNamesObj.end(); ++it) {
        fanNames[it.key()] = it.value().toString();
    }

    beginResetModel();
    qDeleteAll(m_fans);
    m_fans.clear();

    QJsonArray devices = inventory.value(QStringLiteral("devices")).toArray();
    for (const QJsonValue &devVal : devices) {
        QJsonObject device = devVal.toObject();
        QString deviceName = device.value(QStringLiteral("name")).toString();

        QJsonArray fansArray = device.value(QStringLiteral("fans")).toArray();
        for (const QJsonValue &fanVal : fansArray) {
            QJsonObject fan = fanVal.toObject();
            QString fanId = fan.value(QStringLiteral("id")).toString();

            auto *info = new FanStateInfo(this);

            // Basic identity
            info->setFanId(fanId);

            // Display name: friendly_name > label > id
            QString label = fan.value(QStringLiteral("label")).toString(QString());
            QString friendlyName = fanNames.value(fanId, QString());
            QString displayName = friendlyName.isEmpty() ? (label.isEmpty() ? fanId : label) : friendlyName;
            info->setDisplayName(displayName);

            // Support state from inventory
            info->setSupportState(fan.value(QStringLiteral("support_state")).toString(QStringLiteral("unavailable")));
            info->setControlMode(fan.value(QStringLiteral("control_modes")).toArray().first().toString(QString()));
            info->setHasTach(fan.value(QStringLiteral("rpm_feedback")).toBool(false));
            info->setSupportReason(fan.value(QStringLiteral("support_reason")).toString(QString()));

            // Merge runtime state
            if (runtimeMap.contains(fanId)) {
                QJsonObject rt = runtimeMap[fanId];
                QString status = rt.value(QStringLiteral("status")).toString();
                if (status == QStringLiteral("managed")) {
                    info->setState(QStringLiteral("managed"));
                    QJsonObject ctrl = rt.value(QStringLiteral("control")).toObject();
                    info->setTemperatureMillidegrees(ctrl.value(QStringLiteral("aggregated_temp_millidegrees")).toVariant().toLongLong());
                    info->setOutputPercent(ctrl.value(QStringLiteral("logical_output_percent")).toDouble(0.0));
                    bool highTemp = ctrl.value(QStringLiteral("alert_high_temp")).toBool(false);
                    info->setHighTempAlert(highTemp);
                } else if (status == QStringLiteral("degraded")) {
                    info->setState(QStringLiteral("degraded"));
                } else if (status == QStringLiteral("fallback")) {
                    info->setState(QStringLiteral("fallback"));
                } else {
                    info->setState(QStringLiteral("unmanaged"));
                }
            } else {
                info->setState(QStringLiteral("unmanaged"));
            }

            // RPM from inventory (current_rpm)
            if (fan.contains(QStringLiteral("current_rpm")) && !fan.value(QStringLiteral("current_rpm")).isNull()) {
                info->setRpm(fan.value(QStringLiteral("current_rpm")).toInt(0));
            }

            m_fans.append(info);
        }
    }

    // Sort by severity order then by displayName
    std::sort(m_fans.begin(), m_fans.end(), [](FanStateInfo *a, FanStateInfo *b) {
        int sa = severityOrder(a->state(), a->highTempAlert());
        int sb = severityOrder(b->state(), b->highTempAlert());
        if (sa != sb) return sa < sb;
        return a->displayName() < b->displayName();
    });

    endResetModel();
}

int FanListModel::severityOrder(const QString &state, bool highTempAlert)
{
    // Per UI-SPEC: fallback=0, degraded=1, managed+highTemp=2, managed=3, unmanaged=4, partial=5, unavailable=6
    if (state == QStringLiteral("fallback"))   return 0;
    if (state == QStringLiteral("degraded"))    return 1;
    if (state == QStringLiteral("managed") && highTempAlert) return 2;
    if (state == QStringLiteral("managed"))    return 3;
    if (state == QStringLiteral("unmanaged"))  return 4;
    if (state == QStringLiteral("partial"))    return 5;
    if (state == QStringLiteral("unavailable")) return 6;
    return 7; // unknown states sort last
}