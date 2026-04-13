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
#include <QProcessEnvironment>
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
    case FriendlyNameRole: return fan->friendlyName();
    case LabelRole:      return fan->label();
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
        {FriendlyNameRole,     "friendlyName"},
        {LabelRole,            "label"},
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

QVariantMap FanListModel::fanById(const QString &fanId) const
{
    for (FanStateInfo *fan : m_fans) {
        if (fan->fanId() != fanId) {
            continue;
        }

        return QVariantMap{
            {QStringLiteral("fanId"), fan->fanId()},
            {QStringLiteral("displayName"), fan->displayName()},
            {QStringLiteral("friendlyName"), fan->friendlyName()},
            {QStringLiteral("label"), fan->label()},
            {QStringLiteral("supportState"), fan->supportState()},
            {QStringLiteral("controlMode"), fan->controlMode()},
            {QStringLiteral("state"), fan->state()},
            {QStringLiteral("temperatureMillidegrees"), fan->temperatureMillidegrees()},
            {QStringLiteral("rpm"), fan->rpm()},
            {QStringLiteral("outputPercent"), fan->outputPercent()},
            {QStringLiteral("hasTach"), fan->hasTach()},
            {QStringLiteral("supportReason"), fan->supportReason()},
            {QStringLiteral("highTempAlert"), fan->highTempAlert()},
            {QStringLiteral("severityOrder"), severityOrder(fan->state(), fan->highTempAlert())},
        };
    }

    return {};
}

void FanListModel::refresh(const QString &inventoryJson,
                             const QString &runtimeJson,
                             const QString &configJson,
                             const QString &controlJson)
{
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
    QJsonObject controlStatus = QJsonDocument::fromJson(controlJson.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError && !controlJson.isEmpty()) {
        qWarning() << "FanListModel: control status JSON parse error:" << err.errorString();
        return;
    }

    // Build a map from fan_id -> runtime status
    QMap<QString, QJsonObject> runtimeMap;
    QJsonObject fanStatuses = runtime.value(QStringLiteral("fan_statuses")).toObject();
    for (auto it = fanStatuses.begin(); it != fanStatuses.end(); ++it) {
        runtimeMap[it.key()] = it.value().toObject();
    }

    // Build a map from sensor_id -> current temperature
    QMap<QString, qint64> sensorTemps;
    QJsonArray devices = inventory.value(QStringLiteral("devices")).toArray();
    for (const QJsonValue &devVal : devices) {
        QJsonObject device = devVal.toObject();
        QJsonArray temps = device.value(QStringLiteral("temperatures")).toArray();
        for (const QJsonValue &tempVal : temps) {
            QJsonObject sensor = tempVal.toObject();
            if (!sensor.value(QStringLiteral("input_millidegrees_celsius")).isNull()) {
                sensorTemps.insert(
                    sensor.value(QStringLiteral("id")).toString(),
                    sensor.value(QStringLiteral("input_millidegrees_celsius")).toVariant().toLongLong());
            }
        }
    }

    // Draft fan entries — used for temp_sources fallback
    QJsonObject fans = config.value(QStringLiteral("fans")).toObject();

    // For diff detection: build an ordered list of fan IDs from the new data,
    // and a map from fanId -> its new data.
    QStringList newFanIdOrder;
    QMap<QString, QJsonObject> newFanData;
    for (const QJsonValue &devVal : devices) {
        QJsonObject device = devVal.toObject();
        QJsonArray fansArray = device.value(QStringLiteral("fans")).toArray();
        for (const QJsonValue &fanVal : fansArray) {
            QJsonObject fan = fanVal.toObject();
            QString fanId = fan.value(QStringLiteral("id")).toString();
            newFanIdOrder.append(fanId);
            newFanData[fanId] = fan;
        }
    }

    // Build the current fan ID order for comparison
    QStringList currentFanIdOrder;
    for (const FanStateInfo *fan : m_fans) {
        currentFanIdOrder.append(fan->fanId());
    }

    bool structureChanged = (currentFanIdOrder != newFanIdOrder);

    // First load or structural change: full reset
    if (!m_initialized || structureChanged) {
        beginResetModel();
        qDeleteAll(m_fans);
        m_fans.clear();

        for (const QString &fanId : newFanIdOrder) {
            QJsonObject fan = newFanData[fanId];
            auto *info = new FanStateInfo(this);
            info->setFanId(fanId);
            applyFanData(info, fan, runtimeMap, sensorTemps, fans, controlStatus);
            m_fans.append(info);
        }

        std::sort(m_fans.begin(), m_fans.end(), [](FanStateInfo *a, FanStateInfo *b) {
            int sa = severityOrder(a->state(), a->highTempAlert());
            int sb = severityOrder(b->state(), b->highTempAlert());
            if (sa != sb) return sa < sb;
            return a->displayName() < b->displayName();
        });

        endResetModel();
        m_initialized = true;
        return;
    }

    for (int i = 0; i < m_fans.count(); ++i) {
        FanStateInfo *info = m_fans.at(i);
        QJsonObject fan = newFanData.value(info->fanId());

        QString oldDisplayName = info->displayName();
        QString oldFriendlyName = info->friendlyName();
        QString oldLabel = info->label();
        QString oldSupportState = info->supportState();
        QString oldControlMode = info->controlMode();
        QString oldState = info->state();
        qint64 oldTemp = info->temperatureMillidegrees();
        int oldRpm = info->rpm();
        double oldOutput = info->outputPercent();
        bool oldHasTach = info->hasTach();
        QString oldSupportReason = info->supportReason();
        bool oldHighTempAlert = info->highTempAlert();

        applyFanData(info, fan, runtimeMap, sensorTemps, fans, controlStatus);

        QVector<int> changedRoles;
        if (info->displayName() != oldDisplayName)        changedRoles.append(DisplayNameRole);
        if (info->friendlyName() != oldFriendlyName)       changedRoles.append(FriendlyNameRole);
        if (info->label() != oldLabel)                      changedRoles.append(LabelRole);
        if (info->supportState() != oldSupportState)        changedRoles.append(SupportStateRole);
        if (info->controlMode() != oldControlMode)           changedRoles.append(ControlModeRole);
        if (info->state() != oldState)                      changedRoles.append(StateRole);
        if (info->temperatureMillidegrees() != oldTemp)      changedRoles.append(TemperatureMillidegRole);
        if (info->rpm() != oldRpm)                           changedRoles.append(RpmRole);
        if (!qFuzzyCompare(info->outputPercent(), oldOutput)) changedRoles.append(OutputPercentRole);
        if (info->hasTach() != oldHasTach)                   changedRoles.append(HasTachRole);
        if (info->supportReason() != oldSupportReason)       changedRoles.append(SupportReasonRole);
        if (info->highTempAlert() != oldHighTempAlert) {
            changedRoles.append(HighTempAlertRole);
            changedRoles.append(SeverityOrderRole);
        }
        if (info->state() != oldState)
            changedRoles.append(SeverityOrderRole);

        if (!changedRoles.isEmpty()) {
            emit dataChanged(index(i), index(i), changedRoles);
        }
    }

    QList<FanStateInfo *> sorted = m_fans;
    std::sort(sorted.begin(), sorted.end(), [](FanStateInfo *a, FanStateInfo *b) {
        int sa = severityOrder(a->state(), a->highTempAlert());
        int sb = severityOrder(b->state(), b->highTempAlert());
        if (sa != sb) return sa < sb;
        return a->displayName() < b->displayName();
    });

    bool sortOrderChanged = false;
    for (int i = 0; i < m_fans.count(); ++i) {
        if (m_fans.at(i) != sorted.at(i)) {
            sortOrderChanged = true;
            break;
        }
    }

    if (sortOrderChanged) {
        QList<FanStateInfo *> target = sorted;

        for (int targetIdx = 0; targetIdx < target.count(); ++targetIdx) {
            FanStateInfo *item = target.at(targetIdx);

            int currentIdx = -1;
            for (int j = 0; j < m_fans.count(); ++j) {
                if (m_fans.at(j) == item) {
                    currentIdx = j;
                    break;
                }
            }
            if (currentIdx < 0 || currentIdx == targetIdx)
                continue;

            int destChild = targetIdx;
            if (destChild > currentIdx)
                destChild += 1;

            if (beginMoveRows(QModelIndex(), currentIdx, currentIdx, QModelIndex(), destChild)) {
                m_fans.move(currentIdx, targetIdx);
                endMoveRows();
            }
        }
    }
}

void FanListModel::applyFanData(FanStateInfo *info,
                                 const QJsonObject &fan,
                                 const QMap<QString, QJsonObject> &runtimeMap,
                                 const QMap<QString, qint64> &sensorTemps,
                                 const QJsonObject &fans,
                                 const QJsonObject &controlStatus)
{
    QString fanId = info->fanId();

    // Display name: friendly_name > label > id
    QString friendlyName = fan.value(QStringLiteral("friendly_name")).toString(QString());
    QString label = fan.value(QStringLiteral("label")).toString(QString());
    info->setFriendlyName(friendlyName);
    info->setLabel(label);
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
            if (controlStatus.contains(fanId) && controlStatus.value(fanId).isObject()) {
                ctrl = controlStatus.value(fanId).toObject();
            }

            qint64 controlTemp = 0;
            QJsonValue aggregatedTemp = ctrl.value(QStringLiteral("aggregated_temp_millidegrees"));
            if (!aggregatedTemp.isNull()) {
                controlTemp = aggregatedTemp.toVariant().toLongLong();
            } else {
                QStringList sourceIds;
                QJsonArray sensorIds = ctrl.value(QStringLiteral("sensor_ids")).toArray();
                for (const QJsonValue &sensorId : sensorIds) {
                    sourceIds.append(sensorId.toString());
                }
                if (sourceIds.isEmpty() && fans.contains(fanId)) {
                    QJsonArray tempSources = fans.value(fanId).toObject().value(QStringLiteral("temp_sources")).toArray();
                    for (const QJsonValue &sensorId : tempSources) {
                        sourceIds.append(sensorId.toString());
                    }
                }

                QList<qint64> sourceTemps;
                for (const QString &sourceId : sourceIds) {
                    if (sensorTemps.contains(sourceId)) {
                        sourceTemps.append(sensorTemps.value(sourceId));
                    }
                }

                if (!sourceTemps.isEmpty()) {
                    const QString aggregation = ctrl.value(QStringLiteral("aggregation")).toString(
                        fans.value(fanId).toObject().value(QStringLiteral("aggregation")).toString(QStringLiteral("average")));

                    if (aggregation == QStringLiteral("max")) {
                        controlTemp = *std::max_element(sourceTemps.begin(), sourceTemps.end());
                    } else if (aggregation == QStringLiteral("min")) {
                        controlTemp = *std::min_element(sourceTemps.begin(), sourceTemps.end());
                    } else if (aggregation == QStringLiteral("median")) {
                        std::sort(sourceTemps.begin(), sourceTemps.end());
                        controlTemp = sourceTemps.at(sourceTemps.size() / 2);
                    } else {
                        qint64 total = 0;
                        for (qint64 value : sourceTemps) {
                            total += value;
                        }
                        controlTemp = total / sourceTemps.size();
                    }
                }
            }

            info->setTemperatureMillidegrees(controlTemp);
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

    if (qEnvironmentVariableIsSet("KFC_GUI_DEBUG")) {
        qInfo().noquote()
            << QStringLiteral("KFC_GUI_DEBUG fan=%1 state=%2 temp=%3 rpm=%4 output=%5")
                   .arg(info->fanId(),
                        info->state(),
                        QString::number(info->temperatureMillidegrees()),
                        QString::number(info->rpm()),
                        QString::number(info->outputPercent(), 'f', 1));
    }
}

int FanListModel::severityOrder(const QString &state, bool highTempAlert)
{
    if (state == QStringLiteral("fallback"))   return 0;
    if (state == QStringLiteral("degraded"))    return 1;
    if (state == QStringLiteral("managed") && highTempAlert) return 2;
    if (state == QStringLiteral("managed"))    return 3;
    if (state == QStringLiteral("unmanaged"))  return 4;
    if (state == QStringLiteral("partial"))    return 5;
    if (state == QStringLiteral("unavailable")) return 6;
    return 7;
}