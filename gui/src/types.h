/*
 * KDE Fan Control — Value Types for QML
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * QObject-based value types representing fan and sensor state
 * for use in QAbstractListModel subclasses.
 */

#ifndef TYPES_H
#define TYPES_H

#include <QObject>
#include <QString>

class FanStateInfo : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString fanId READ fanId WRITE setFanId NOTIFY fanIdChanged)
    Q_PROPERTY(QString displayName READ displayName WRITE setDisplayName NOTIFY displayNameChanged)
    Q_PROPERTY(QString friendlyName READ friendlyName WRITE setFriendlyName NOTIFY friendlyNameChanged)
    Q_PROPERTY(QString label READ label WRITE setLabel NOTIFY labelChanged)
    Q_PROPERTY(QString supportState READ supportState WRITE setSupportState NOTIFY supportStateChanged)
    Q_PROPERTY(QString controlMode READ controlMode WRITE setControlMode NOTIFY controlModeChanged)
    Q_PROPERTY(QString state READ state WRITE setState NOTIFY stateChanged)
    Q_PROPERTY(qint64 temperatureMillidegrees READ temperatureMillidegrees WRITE setTemperatureMillidegrees NOTIFY temperatureMillidegreesChanged)
    Q_PROPERTY(int rpm READ rpm WRITE setRpm NOTIFY rpmChanged)
    Q_PROPERTY(double outputPercent READ outputPercent WRITE setOutputPercent NOTIFY outputPercentChanged)
    Q_PROPERTY(bool hasTach READ hasTach WRITE setHasTach NOTIFY hasTachChanged)
    Q_PROPERTY(QString supportReason READ supportReason WRITE setSupportReason NOTIFY supportReasonChanged)
    Q_PROPERTY(bool highTempAlert READ highTempAlert WRITE setHighTempAlert NOTIFY highTempAlertChanged)

public:
    explicit FanStateInfo(QObject *parent = nullptr) : QObject(parent) {}

    QString fanId() const { return m_fanId; }
    QString displayName() const { return m_displayName; }
    QString friendlyName() const { return m_friendlyName; }
    QString label() const { return m_label; }
    QString supportState() const { return m_supportState; }
    QString controlMode() const { return m_controlMode; }
    QString state() const { return m_state; }
    qint64 temperatureMillidegrees() const { return m_temperatureMillidegrees; }
    int rpm() const { return m_rpm; }
    double outputPercent() const { return m_outputPercent; }
    bool hasTach() const { return m_hasTach; }
    QString supportReason() const { return m_supportReason; }
    bool highTempAlert() const { return m_highTempAlert; }

    void setFanId(const QString &v) { if (m_fanId != v) { m_fanId = v; Q_EMIT fanIdChanged(); } }
    void setDisplayName(const QString &v) { if (m_displayName != v) { m_displayName = v; Q_EMIT displayNameChanged(); } }
    void setFriendlyName(const QString &v) { if (m_friendlyName != v) { m_friendlyName = v; Q_EMIT friendlyNameChanged(); } }
    void setLabel(const QString &v) { if (m_label != v) { m_label = v; Q_EMIT labelChanged(); } }
    void setSupportState(const QString &v) { if (m_supportState != v) { m_supportState = v; Q_EMIT supportStateChanged(); } }
    void setControlMode(const QString &v) { if (m_controlMode != v) { m_controlMode = v; Q_EMIT controlModeChanged(); } }
    void setState(const QString &v) { if (m_state != v) { m_state = v; Q_EMIT stateChanged(); } }
    void setTemperatureMillidegrees(qint64 v) { if (m_temperatureMillidegrees != v) { m_temperatureMillidegrees = v; Q_EMIT temperatureMillidegreesChanged(); } }
    void setRpm(int v) { if (m_rpm != v) { m_rpm = v; Q_EMIT rpmChanged(); } }
    void setOutputPercent(double v) { if (!qFuzzyCompare(m_outputPercent, v)) { m_outputPercent = v; Q_EMIT outputPercentChanged(); } }
    void setHasTach(bool v) { if (m_hasTach != v) { m_hasTach = v; Q_EMIT hasTachChanged(); } }
    void setSupportReason(const QString &v) { if (m_supportReason != v) { m_supportReason = v; Q_EMIT supportReasonChanged(); } }
    void setHighTempAlert(bool v) { if (m_highTempAlert != v) { m_highTempAlert = v; Q_EMIT highTempAlertChanged(); } }

signals:
    void fanIdChanged();
    void displayNameChanged();
    void friendlyNameChanged();
    void labelChanged();
    void supportStateChanged();
    void controlModeChanged();
    void stateChanged();
    void temperatureMillidegreesChanged();
    void rpmChanged();
    void outputPercentChanged();
    void hasTachChanged();
    void supportReasonChanged();
    void highTempAlertChanged();

private:
    QString m_fanId;
    QString m_displayName;
    QString m_friendlyName;
    QString m_label;
    QString m_supportState;
    QString m_controlMode;
    QString m_state;
    qint64 m_temperatureMillidegrees = 0;
    int m_rpm = 0;
    double m_outputPercent = 0.0;
    bool m_hasTach = false;
    QString m_supportReason;
    bool m_highTempAlert = false;
};

class SensorInfo : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString sensorId READ sensorId WRITE setSensorId NOTIFY sensorIdChanged)
    Q_PROPERTY(QString displayName READ displayName WRITE setDisplayName NOTIFY displayNameChanged)
    Q_PROPERTY(QString friendlyName READ friendlyName WRITE setFriendlyName NOTIFY friendlyNameChanged)
    Q_PROPERTY(QString label READ label WRITE setLabel NOTIFY labelChanged)
    Q_PROPERTY(qint64 currentTemperatureMillidegrees READ currentTemperatureMillidegrees WRITE setCurrentTemperatureMillidegrees NOTIFY currentTemperatureMillidegreesChanged)
    Q_PROPERTY(QString deviceName READ deviceName WRITE setDeviceName NOTIFY deviceNameChanged)
    Q_PROPERTY(QString sourcePath READ sourcePath WRITE setSourcePath NOTIFY sourcePathChanged)

public:
    explicit SensorInfo(QObject *parent = nullptr) : QObject(parent) {}

    QString sensorId() const { return m_sensorId; }
    QString displayName() const { return m_displayName; }
    QString friendlyName() const { return m_friendlyName; }
    QString label() const { return m_label; }
    qint64 currentTemperatureMillidegrees() const { return m_currentTemperatureMillidegrees; }
    QString deviceName() const { return m_deviceName; }
    QString sourcePath() const { return m_sourcePath; }

    void setSensorId(const QString &v) { if (m_sensorId != v) { m_sensorId = v; Q_EMIT sensorIdChanged(); } }
    void setDisplayName(const QString &v) { if (m_displayName != v) { m_displayName = v; Q_EMIT displayNameChanged(); } }
    void setFriendlyName(const QString &v) { if (m_friendlyName != v) { m_friendlyName = v; Q_EMIT friendlyNameChanged(); } }
    void setLabel(const QString &v) { if (m_label != v) { m_label = v; Q_EMIT labelChanged(); } }
    void setCurrentTemperatureMillidegrees(qint64 v) { if (m_currentTemperatureMillidegrees != v) { m_currentTemperatureMillidegrees = v; Q_EMIT currentTemperatureMillidegreesChanged(); } }
    void setDeviceName(const QString &v) { if (m_deviceName != v) { m_deviceName = v; Q_EMIT deviceNameChanged(); } }
    void setSourcePath(const QString &v) { if (m_sourcePath != v) { m_sourcePath = v; Q_EMIT sourcePathChanged(); } }

signals:
    void sensorIdChanged();
    void displayNameChanged();
    void friendlyNameChanged();
    void labelChanged();
    void currentTemperatureMillidegreesChanged();
    void deviceNameChanged();
    void sourcePathChanged();

private:
    QString m_sensorId;
    QString m_displayName;
    QString m_friendlyName;
    QString m_label;
    qint64 m_currentTemperatureMillidegrees = 0;
    QString m_deviceName;
    QString m_sourcePath;
};

// --- Helper conversion functions ---

double millidegreesToCelsius(qint64 millidegrees);
QString formatTemperature(double celsius);
QString formatRpm(int rpm);
QString formatOutputPercent(double percent);

#endif // TYPES_H