/*
 * KDE Fan Control — Fan List Model
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * QAbstractListModel for the fan overview dashboard.
 * Merges inventory, runtime state, and draft config JSON
 * into rows of FanStateInfo objects sorted by severity order.
 *
 * Uses diff-based updates: value-only changes emit dataChanged()
 * so QML delegates update in-place without destruction/recreation.
 * Structural changes (fan added/removed/reordered) fall back to
 * beginResetModel/endResetModel.
 */

#ifndef FAN_LIST_MODEL_H
#define FAN_LIST_MODEL_H

#include <QAbstractListModel>
#include <QList>
#include <QVariantMap>
#include "../types.h"

class FanListModel : public QAbstractListModel
{
    Q_OBJECT
    Q_ENUMS(Roles)

public:
    enum Roles {
        FanIdRole = Qt::UserRole + 1,
        DisplayNameRole,
        FriendlyNameRole,
        LabelRole,
        SupportStateRole,
        ControlModeRole,
        StateRole,
        TemperatureMillidegRole,
        RpmRole,
        OutputPercentRole,
        HasTachRole,
        SupportReasonRole,
        HighTempAlertRole,
        SeverityOrderRole
    };

    explicit FanListModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = QModelIndex()) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    Q_INVOKABLE void refresh(const QString &inventoryJson,
                              const QString &runtimeJson,
                              const QString &configJson,
                              const QString &controlJson = QString());
    Q_INVOKABLE QVariantMap fanById(const QString &fanId) const;

private:
    static int severityOrder(const QString &state, bool highTempAlert);

    void applyFanData(FanStateInfo *info,
                      const QJsonObject &fan,
                      const QMap<QString, QJsonObject> &runtimeMap,
                      const QMap<QString, qint64> &sensorTemps,
                      const QJsonObject &fans,
                      const QJsonObject &controlStatus);

    QList<FanStateInfo *> m_fans;
    bool m_initialized = false;
};

#endif // FAN_LIST_MODEL_H