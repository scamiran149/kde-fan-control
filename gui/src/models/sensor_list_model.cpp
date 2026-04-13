/*
 * KDE Fan Control — Sensor List Model Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "sensor_list_model.h"

#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QDebug>

SensorListModel::SensorListModel(QObject *parent)
    : QAbstractListModel(parent)
{
}

int SensorListModel::rowCount(const QModelIndex &parent) const
{
    if (parent.isValid())
        return 0;
    return m_sensors.count();
}

QVariant SensorListModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_sensors.count())
        return QVariant();

    SensorInfo *sensor = m_sensors.at(index.row());

    switch (role) {
    case SensorIdRole:           return sensor->sensorId();
    case DisplayNameRole:        return sensor->displayName();
    case FriendlyNameRole:       return sensor->friendlyName();
    case LabelRole:              return sensor->label();
    case TemperatureMillidegRole: return sensor->currentTemperatureMillidegrees();
    case DeviceNameRole:          return sensor->deviceName();
    case SourcePathRole:          return sensor->sourcePath();
    case IsFanSourceRole:         return false;
    default:                     return QVariant();
    }
}

QHash<int, QByteArray> SensorListModel::roleNames() const
{
    static const QHash<int, QByteArray> roles = {
        {SensorIdRole,            "sensorId"},
        {DisplayNameRole,         "displayName"},
        {FriendlyNameRole,        "friendlyName"},
        {LabelRole,               "label"},
        {TemperatureMillidegRole, "temperatureMillidegrees"},
        {DeviceNameRole,          "deviceName"},
        {SourcePathRole,          "sourcePath"},
        {IsFanSourceRole,         "isFanSource"}
    };
    return roles;
}

void SensorListModel::refresh(const QString &inventoryJson)
{
    QJsonParseError err;
    QJsonObject inventory = QJsonDocument::fromJson(inventoryJson.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError && !inventoryJson.isEmpty()) {
        qWarning() << "SensorListModel: JSON parse error:" << err.errorString();
        return;
    }

    // Build new sensor ID list and data maps
    QStringList newSensorIdOrder;
    QMap<QString, QJsonObject> newSensorData;
    QMap<QString, QString> newSensorDevice;
    QJsonArray devices = inventory.value(QStringLiteral("devices")).toArray();
    for (const QJsonValue &devVal : devices) {
        QJsonObject device = devVal.toObject();
        QString deviceName = device.value(QStringLiteral("name")).toString();

        QJsonArray temps = device.value(QStringLiteral("temperatures")).toArray();
        for (const QJsonValue &tempVal : temps) {
            QJsonObject sensor = tempVal.toObject();
            QString sensorId = sensor.value(QStringLiteral("id")).toString();
            newSensorIdOrder.append(sensorId);
            newSensorData[sensorId] = sensor;
            newSensorDevice[sensorId] = deviceName;
        }
    }

    // Current sensor ID list
    QStringList currentSensorIdOrder;
    for (const SensorInfo *sensor : m_sensors) {
        currentSensorIdOrder.append(sensor->sensorId());
    }

    bool structureChanged = (currentSensorIdOrder != newSensorIdOrder);

    if (!m_initialized || structureChanged) {
        beginResetModel();
        qDeleteAll(m_sensors);
        m_sensors.clear();

        for (const QString &sensorId : newSensorIdOrder) {
            QJsonObject sensor = newSensorData[sensorId];
            QString deviceName = newSensorDevice[sensorId];

            auto *info = new SensorInfo(this);
            info->setSensorId(sensorId);
            info->setDeviceName(deviceName);

            QString friendlyName = sensor.value(QStringLiteral("friendly_name")).toString(QString());
            QString label = sensor.value(QStringLiteral("label")).toString(QString());
            info->setFriendlyName(friendlyName);
            info->setLabel(label);
            QString displayName = friendlyName.isEmpty() ? (label.isEmpty() ? sensorId : label) : friendlyName;
            info->setDisplayName(displayName);

            if (sensor.contains(QStringLiteral("input_millidegrees_celsius"))
                && !sensor.value(QStringLiteral("input_millidegrees_celsius")).isNull()) {
                info->setCurrentTemperatureMillidegrees(
                    sensor.value(QStringLiteral("input_millidegrees_celsius")).toVariant().toLongLong());
            }

            info->setSourcePath(sensor.value(QStringLiteral("sysfs_path")).toString(QString()));

            m_sensors.append(info);
        }

        endResetModel();
        m_initialized = true;
        return;
    }

    // Same sensors, same order — update values in place
    static const QVector<int> allRoles = {
        DisplayNameRole, FriendlyNameRole, LabelRole,
        TemperatureMillidegRole, DeviceNameRole, SourcePathRole
    };

    int firstChanged = -1;
    int lastChanged = -1;

    for (int i = 0; i < m_sensors.count(); ++i) {
        SensorInfo *info = m_sensors.at(i);
        QJsonObject sensor = newSensorData.value(info->sensorId());

        qint64 oldTemp = info->currentTemperatureMillidegrees();
        QString oldFriendlyName = info->friendlyName();
        QString oldLabel = info->label();
        QString oldDisplayName = info->displayName();

        QString friendlyName = sensor.value(QStringLiteral("friendly_name")).toString(QString());
        QString label = sensor.value(QStringLiteral("label")).toString(QString());
        info->setFriendlyName(friendlyName);
        info->setLabel(label);
        QString displayName = friendlyName.isEmpty() ? (label.isEmpty() ? info->sensorId() : label) : friendlyName;
        info->setDisplayName(displayName);

        if (sensor.contains(QStringLiteral("input_millidegrees_celsius"))
            && !sensor.value(QStringLiteral("input_millidegrees_celsius")).isNull()) {
            info->setCurrentTemperatureMillidegrees(
                sensor.value(QStringLiteral("input_millidegrees_celsius")).toVariant().toLongLong());
        }

        bool changed = (info->currentTemperatureMillidegrees() != oldTemp
                        || info->friendlyName() != oldFriendlyName
                        || info->label() != oldLabel
                        || info->displayName() != oldDisplayName);

        if (changed) {
            if (firstChanged < 0) firstChanged = i;
            lastChanged = i;
        }
    }

    if (firstChanged >= 0) {
        emit dataChanged(index(firstChanged), index(lastChanged), allRoles);
    }
}