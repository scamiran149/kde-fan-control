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
    case TemperatureMillidegRole: return sensor->currentTemperatureMillidegrees();
    case DeviceNameRole:          return sensor->deviceName();
    case SourcePathRole:          return sensor->sourcePath();
    case IsFanSourceRole:         return false; // Will be set based on draft config
    default:                     return QVariant();
    }
}

QHash<int, QByteArray> SensorListModel::roleNames() const
{
    static const QHash<int, QByteArray> roles = {
        {SensorIdRole,            "sensorId"},
        {DisplayNameRole,         "displayName"},
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

    beginResetModel();
    qDeleteAll(m_sensors);
    m_sensors.clear();

    QJsonArray devices = inventory.value(QStringLiteral("devices")).toArray();
    for (const QJsonValue &devVal : devices) {
        QJsonObject device = devVal.toObject();
        QString deviceName = device.value(QStringLiteral("name")).toString();

        QJsonArray temps = device.value(QStringLiteral("temperatures")).toArray();
        for (const QJsonValue &tempVal : temps) {
            QJsonObject sensor = tempVal.toObject();

            auto *info = new SensorInfo(this);
            info->setSensorId(sensor.value(QStringLiteral("id")).toString());
            info->setDeviceName(deviceName);

            // Display name: friendly_name > label > id
            QString friendlyName = sensor.value(QStringLiteral("friendly_name")).toString(QString());
            QString label = sensor.value(QStringLiteral("label")).toString(QString());
            QString displayName = friendlyName.isEmpty() ? (label.isEmpty() ? info->sensorId() : label) : friendlyName;
            info->setDisplayName(displayName);

            // Temperature
            if (sensor.contains(QStringLiteral("input_millidegrees_celsius"))
                && !sensor.value(QStringLiteral("input_millidegrees_celsius")).isNull()) {
                info->setCurrentTemperatureMillidegrees(
                    sensor.value(QStringLiteral("input_millidegrees_celsius")).toVariant().toLongLong());
            }

            info->setSourcePath(device.value(QStringLiteral("sysfs_path")).toString());

            m_sensors.append(info);
        }
    }

    endResetModel();
}