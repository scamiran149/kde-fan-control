/*
 * KDE Fan Control — Fan List Model
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * QAbstractListModel for the fan overview dashboard.
 * Merges inventory, runtime state, and draft config JSON
 * into rows of FanStateInfo objects sorted by severity order.
 */

#ifndef FAN_LIST_MODEL_H
#define FAN_LIST_MODEL_H

#include <QAbstractListModel>
#include <QList>
#include "../types.h"

class FanListModel : public QAbstractListModel
{
    Q_OBJECT

public:
    enum Roles {
        FanIdRole = Qt::UserRole + 1,
        DisplayNameRole,
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
                              const QString &configJson);

private:
    static int severityOrder(const QString &state, bool highTempAlert);

    QList<FanStateInfo *> m_fans;
};

#endif // FAN_LIST_MODEL_H