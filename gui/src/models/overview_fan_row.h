/*
 * KDE Fan Control — Overview Fan Row
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Stable per-fan row object for the overview list.
 * Properties are split into structural (rarely change) and
 * telemetry (fast path, update in place).
 *
 * QML delegates should bind directly to these properties for
 * surgical per-property updates without model-level dataChanged.
 */

#ifndef OVERVIEW_FAN_ROW_H
#define OVERVIEW_FAN_ROW_H

#include <QObject>
#include <QString>

class OverviewFanRow : public QObject
{
    Q_OBJECT

    Q_PROPERTY(QString fanId READ fanId WRITE setFanId NOTIFY fanIdChanged)
    Q_PROPERTY(QString displayName READ displayName WRITE setDisplayName NOTIFY displayNameChanged)
    Q_PROPERTY(QString friendlyName READ friendlyName WRITE setFriendlyName NOTIFY friendlyNameChanged)
    Q_PROPERTY(QString hardwareLabel READ hardwareLabel WRITE setHardwareLabel NOTIFY hardwareLabelChanged)
    Q_PROPERTY(QString supportState READ supportState WRITE setSupportState NOTIFY supportStateChanged)
    Q_PROPERTY(QString controlMode READ controlMode WRITE setControlMode NOTIFY controlModeChanged)
    Q_PROPERTY(bool hasTach READ hasTach WRITE setHasTach NOTIFY hasTachChanged)
    Q_PROPERTY(QString supportReason READ supportReason WRITE setSupportReason NOTIFY supportReasonChanged)
    Q_PROPERTY(QString orderingBucket READ orderingBucket WRITE setOrderingBucket NOTIFY orderingBucketChanged)
    Q_PROPERTY(QString stateText READ stateText WRITE setStateText NOTIFY stateTextChanged)
    Q_PROPERTY(QString stateIconName READ stateIconName WRITE setStateIconName NOTIFY stateIconNameChanged)
    Q_PROPERTY(QString stateColor READ stateColor WRITE setStateColor NOTIFY stateColorChanged)
    Q_PROPERTY(bool showSupportReason READ showSupportReason WRITE setShowSupportReason NOTIFY showSupportReasonChanged)

    Q_PROPERTY(qint64 temperatureMillidegrees READ temperatureMillidegrees WRITE setTemperatureMillidegrees NOTIFY temperatureMillidegreesChanged)
    Q_PROPERTY(QString temperatureText READ temperatureText WRITE setTemperatureText NOTIFY temperatureTextChanged)
    Q_PROPERTY(int rpm READ rpm WRITE setRpm NOTIFY rpmChanged)
    Q_PROPERTY(QString rpmText READ rpmText WRITE setRpmText NOTIFY rpmTextChanged)
    Q_PROPERTY(double outputPercent READ outputPercent WRITE setOutputPercent NOTIFY outputPercentChanged)
    Q_PROPERTY(QString outputText READ outputText WRITE setOutputText NOTIFY outputTextChanged)
    Q_PROPERTY(double outputFillRatio READ outputFillRatio WRITE setOutputFillRatio NOTIFY outputFillRatioChanged)
    Q_PROPERTY(bool highTempAlert READ highTempAlert WRITE setHighTempAlert NOTIFY highTempAlertChanged)
    Q_PROPERTY(bool showRpm READ showRpm WRITE setShowRpm NOTIFY showRpmChanged)
    Q_PROPERTY(bool showOutput READ showOutput WRITE setShowOutput NOTIFY showOutputChanged)
    Q_PROPERTY(QString visualState READ visualState WRITE setVisualState NOTIFY visualStateChanged)

public:
    explicit OverviewFanRow(QObject *parent = nullptr) : QObject(parent) {}

    QString fanId() const { return m_fanId; }
    QString displayName() const { return m_displayName; }
    QString friendlyName() const { return m_friendlyName; }
    QString hardwareLabel() const { return m_hardwareLabel; }
    QString supportState() const { return m_supportState; }
    QString controlMode() const { return m_controlMode; }
    bool hasTach() const { return m_hasTach; }
    QString supportReason() const { return m_supportReason; }
    QString orderingBucket() const { return m_orderingBucket; }
    QString stateText() const { return m_stateText; }
    QString stateIconName() const { return m_stateIconName; }
    QString stateColor() const { return m_stateColor; }
    bool showSupportReason() const { return m_showSupportReason; }

    qint64 temperatureMillidegrees() const { return m_temperatureMillidegrees; }
    QString temperatureText() const { return m_temperatureText; }
    int rpm() const { return m_rpm; }
    QString rpmText() const { return m_rpmText; }
    double outputPercent() const { return m_outputPercent; }
    QString outputText() const { return m_outputText; }
    double outputFillRatio() const { return m_outputFillRatio; }
    bool highTempAlert() const { return m_highTempAlert; }
    bool showRpm() const { return m_showRpm; }
    bool showOutput() const { return m_showOutput; }
    QString visualState() const { return m_visualState; }

    void setFanId(const QString &v) { if (m_fanId != v) { m_fanId = v; Q_EMIT fanIdChanged(); } }
    void setDisplayName(const QString &v) { if (m_displayName != v) { m_displayName = v; Q_EMIT displayNameChanged(); } }
    void setFriendlyName(const QString &v) { if (m_friendlyName != v) { m_friendlyName = v; Q_EMIT friendlyNameChanged(); } }
    void setHardwareLabel(const QString &v) { if (m_hardwareLabel != v) { m_hardwareLabel = v; Q_EMIT hardwareLabelChanged(); } }
    void setSupportState(const QString &v) { if (m_supportState != v) { m_supportState = v; Q_EMIT supportStateChanged(); } }
    void setControlMode(const QString &v) { if (m_controlMode != v) { m_controlMode = v; Q_EMIT controlModeChanged(); } }
    void setHasTach(bool v) { if (m_hasTach != v) { m_hasTach = v; Q_EMIT hasTachChanged(); } }
    void setSupportReason(const QString &v) { if (m_supportReason != v) { m_supportReason = v; Q_EMIT supportReasonChanged(); } }
    void setOrderingBucket(const QString &v) { if (m_orderingBucket != v) { m_orderingBucket = v; Q_EMIT orderingBucketChanged(); } }
    void setStateText(const QString &v) { if (m_stateText != v) { m_stateText = v; Q_EMIT stateTextChanged(); } }
    void setStateIconName(const QString &v) { if (m_stateIconName != v) { m_stateIconName = v; Q_EMIT stateIconNameChanged(); } }
    void setStateColor(const QString &v) { if (m_stateColor != v) { m_stateColor = v; Q_EMIT stateColorChanged(); } }
    void setShowSupportReason(bool v) { if (m_showSupportReason != v) { m_showSupportReason = v; Q_EMIT showSupportReasonChanged(); } }

    void setTemperatureMillidegrees(qint64 v) { if (m_temperatureMillidegrees != v) { m_temperatureMillidegrees = v; Q_EMIT temperatureMillidegreesChanged(); } }
    void setTemperatureText(const QString &v) { if (m_temperatureText != v) { m_temperatureText = v; Q_EMIT temperatureTextChanged(); } }
    void setRpm(int v) { if (m_rpm != v) { m_rpm = v; Q_EMIT rpmChanged(); } }
    void setRpmText(const QString &v) { if (m_rpmText != v) { m_rpmText = v; Q_EMIT rpmTextChanged(); } }
    void setOutputPercent(double v) { if (!qFuzzyCompare(m_outputPercent, v)) { m_outputPercent = v; Q_EMIT outputPercentChanged(); } }
    void setOutputText(const QString &v) { if (m_outputText != v) { m_outputText = v; Q_EMIT outputTextChanged(); } }
    void setOutputFillRatio(double v) { if (!qFuzzyCompare(m_outputFillRatio, v)) { m_outputFillRatio = v; Q_EMIT outputFillRatioChanged(); } }
    void setHighTempAlert(bool v) { if (m_highTempAlert != v) { m_highTempAlert = v; Q_EMIT highTempAlertChanged(); } }
    void setShowRpm(bool v) { if (m_showRpm != v) { m_showRpm = v; Q_EMIT showRpmChanged(); } }
    void setShowOutput(bool v) { if (m_showOutput != v) { m_showOutput = v; Q_EMIT showOutputChanged(); } }
    void setVisualState(const QString &v) { if (m_visualState != v) { m_visualState = v; Q_EMIT visualStateChanged(); } }

signals:
    void fanIdChanged();
    void displayNameChanged();
    void friendlyNameChanged();
    void hardwareLabelChanged();
    void supportStateChanged();
    void controlModeChanged();
    void hasTachChanged();
    void supportReasonChanged();
    void orderingBucketChanged();
    void stateTextChanged();
    void stateIconNameChanged();
    void stateColorChanged();
    void showSupportReasonChanged();
    void temperatureMillidegreesChanged();
    void temperatureTextChanged();
    void rpmChanged();
    void rpmTextChanged();
    void outputPercentChanged();
    void outputTextChanged();
    void outputFillRatioChanged();
    void highTempAlertChanged();
    void showRpmChanged();
    void showOutputChanged();
    void visualStateChanged();

private:
    QString m_fanId;
    QString m_displayName;
    QString m_friendlyName;
    QString m_hardwareLabel;
    QString m_supportState;
    QString m_controlMode;
    bool m_hasTach = false;
    QString m_supportReason;
    QString m_orderingBucket;
    QString m_stateText;
    QString m_stateIconName;
    QString m_stateColor;
    bool m_showSupportReason = false;

    qint64 m_temperatureMillidegrees = 0;
    QString m_temperatureText;
    int m_rpm = 0;
    QString m_rpmText;
    double m_outputPercent = 0.0;
    QString m_outputText;
    double m_outputFillRatio = 0.0;
    bool m_highTempAlert = false;
    bool m_showRpm = false;
    bool m_showOutput = false;
    QString m_visualState;
};

#endif // OVERVIEW_FAN_ROW_H