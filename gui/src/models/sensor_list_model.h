/*
 * KDE Fan Control — Sensor List Model
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * QAbstractListModel for the sensor inventory listing.
 * Parses InventorySnapshot JSON from the daemon.
 *
 * Uses diff-based updates: value-only changes emit dataChanged()
 * so QML delegates update in-place. Structural changes
 * (sensor added/removed) fall back to beginResetModel/endResetModel.
 */

#ifndef SENSOR_LIST_MODEL_H
#define SENSOR_LIST_MODEL_H

#include <QAbstractListModel>
#include <QList>
#include "../types.h"

class SensorListModel : public QAbstractListModel
{
    Q_OBJECT
    Q_ENUMS(Roles)

public:
    enum Roles {
        SensorIdRole = Qt::UserRole + 1,
        DisplayNameRole,
        FriendlyNameRole,
        LabelRole,
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
    bool m_initialized = false;
};

#endif // SENSOR_LIST_MODEL_H