/*
 * KDE Fan Control — Overview Model
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Purpose-built overview model with split structure/telemetry paths.
 * Structure updates may add/remove/reorder rows.
 * Telemetry updates only write live properties on existing rows.
 *
 * Exposes a RowObjectRole so QML can bind directly to OverviewFanRow
 * properties for surgical per-property updates without dataChanged cascades.
 */

#ifndef OVERVIEW_MODEL_H
#define OVERVIEW_MODEL_H

#include <QAbstractListModel>
#include <QList>
#include <QMap>
#include <QJsonObject>
#include "overview_fan_row.h"

class OverviewModel : public QAbstractListModel
{
    Q_OBJECT

public:
    enum Roles {
        RowObjectRole = Qt::UserRole + 1,
        FanIdRole,
        DisplayNameRole,
        OrderingBucketRole,
        VisualStateRole,
    };

    explicit OverviewModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = QModelIndex()) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    Q_INVOKABLE void applyStructure(const QString &structureJson);
    Q_INVOKABLE void applyTelemetry(const QString &telemetryJson);

    Q_INVOKABLE OverviewFanRow *rowByFanId(const QString &fanId) const;

private:
    void applyStructureToRow(OverviewFanRow *row, const QJsonObject &obj);
    void applyTelemetryToRow(OverviewFanRow *row, const QJsonObject &obj);

    QList<OverviewFanRow *> m_rows;
    QMap<QString, OverviewFanRow *> m_index;
    bool m_initialized = false;
};

#endif // OVERVIEW_MODEL_H