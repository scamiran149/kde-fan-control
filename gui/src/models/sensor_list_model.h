/*
 * KDE Fan Control — Sensor List Model
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * QAbstractListModel for the sensor inventory listing.
 * Parses InventorySnapshot JSON from the daemon.
 */

#ifndef SENSOR_LIST_MODEL_H
#define SENSOR_LIST_MODEL_H

#include <QAbstractListModel>
#include <QList>
#include "../types.h"

class SensorListModel : public QAbstractListModel
{
    Q_OBJECT

public:
    enum Roles {
        SensorIdRole = Qt::UserRole + 1,
        DisplayNameRole,
        TemperatureMillidegRole,
        DeviceNameRole,
        SourcePathRole,
        IsFanSourceRole
    };

    explicit SensorListModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = QModelIndex()) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    Q_INVOKABLE void refresh(const QString &inventoryJson);

private:
    QList<SensorInfo *> m_sensors;
};

#endif // SENSOR_LIST_MODEL_H