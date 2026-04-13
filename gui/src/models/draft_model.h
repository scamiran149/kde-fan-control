/*
 * KDE Fan Control — Draft Editing Model
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Manages draft configuration state for a selected fan, including
 * enrollment, sensor sources, PID gains, validation errors, and
 * auto-tune proposal flow. All write operations go through
 * DaemonInterface to the daemon's DBus API.
 */

#ifndef DRAFT_MODEL_H
#define DRAFT_MODEL_H

#include <QObject>
#include <QString>
#include <QStringList>
#include <QJsonObject>

class DaemonInterface;

class DraftModel : public QObject
{
    Q_OBJECT

    Q_PROPERTY(QString fanId READ fanId WRITE setFanId NOTIFY fanIdChanged)
    Q_PROPERTY(bool enrolled READ enrolled WRITE setEnrolled NOTIFY enrolledChanged)
    Q_PROPERTY(QString controlMode READ controlMode WRITE setControlMode NOTIFY controlModeChanged)
    Q_PROPERTY(QStringList sensorIds READ sensorIds WRITE setSensorIds NOTIFY sensorIdsChanged)
    Q_PROPERTY(QString aggregation READ aggregation WRITE setAggregation NOTIFY aggregationChanged)
    Q_PROPERTY(double targetTempCelsius READ targetTempCelsius WRITE setTargetTempCelsius NOTIFY targetTempCelsiusChanged)
    Q_PROPERTY(double targetTempMillidegrees READ targetTempMillidegrees NOTIFY targetTempCelsiusChanged)
    Q_PROPERTY(double kp READ kp NOTIFY pidGainsChanged)
    Q_PROPERTY(double ki READ ki NOTIFY pidGainsChanged)
    Q_PROPERTY(double kd READ kd NOTIFY pidGainsChanged)
    Q_PROPERTY(bool autoTuneRunning READ autoTuneRunning NOTIFY autoTuneRunningChanged)
    Q_PROPERTY(bool autoTuneProposalAvailable READ autoTuneProposalAvailable NOTIFY autoTuneProposalAvailableChanged)
    Q_PROPERTY(double proposedKp READ proposedKp NOTIFY autoTuneProposalChanged)
    Q_PROPERTY(double proposedKi READ proposedKi NOTIFY autoTuneProposalChanged)
    Q_PROPERTY(double proposedKd READ proposedKd NOTIFY autoTuneProposalChanged)
    Q_PROPERTY(bool hasValidationError READ hasValidationError NOTIFY validationStateChanged)
    Q_PROPERTY(QStringList validationErrors READ validationErrors NOTIFY validationStateChanged)
    Q_PROPERTY(bool hasApplyError READ hasApplyError NOTIFY applyStateChanged)
    Q_PROPERTY(QStringList applyErrors READ applyErrors NOTIFY applyStateChanged)

    // Advanced controls (cadence, deadband, actuator policy)
    Q_PROPERTY(int sampleIntervalMs READ sampleIntervalMs NOTIFY advancedControlsChanged)
    Q_PROPERTY(int controlIntervalMs READ controlIntervalMs NOTIFY advancedControlsChanged)
    Q_PROPERTY(int writeIntervalMs READ writeIntervalMs NOTIFY advancedControlsChanged)
    Q_PROPERTY(int deadbandMillidegrees READ deadbandMillidegrees NOTIFY advancedControlsChanged)
    Q_PROPERTY(double outputMinPercent READ outputMinPercent NOTIFY advancedControlsChanged)
    Q_PROPERTY(double outputMaxPercent READ outputMaxPercent NOTIFY advancedControlsChanged)

public:
    explicit DraftModel(DaemonInterface *daemon, QObject *parent = nullptr);

    QString fanId() const { return m_fanId; }
    void setFanId(const QString &id);

    bool enrolled() const { return m_enrolled; }
    void setEnrolled(bool enrolled);

    QString controlMode() const { return m_controlMode; }
    void setControlMode(const QString &mode);

    QStringList sensorIds() const { return m_sensorIds; }
    void setSensorIds(const QStringList &ids);

    QString aggregation() const { return m_aggregation; }
    void setAggregation(const QString &agg);

    double targetTempCelsius() const { return m_targetTempMillidegrees / 1000.0; }
    void setTargetTempCelsius(double celsius);

    double targetTempMillidegrees() const { return m_targetTempMillidegrees; }

    double kp() const { return m_kp; }
    double ki() const { return m_ki; }
    double kd() const { return m_kd; }

    bool autoTuneRunning() const { return m_autoTuneRunning; }
    bool autoTuneProposalAvailable() const { return m_autoTuneProposalAvailable; }

    double proposedKp() const { return m_proposedKp; }
    double proposedKi() const { return m_proposedKi; }
    double proposedKd() const { return m_proposedKd; }

    bool hasValidationError() const { return m_hasValidationError; }
    QStringList validationErrors() const { return m_validationErrors; }

    bool hasApplyError() const { return m_hasApplyError; }
    QStringList applyErrors() const { return m_applyErrors; }

    // Advanced controls getters
    int sampleIntervalMs() const { return m_sampleIntervalMs; }
    int controlIntervalMs() const { return m_controlIntervalMs; }
    int writeIntervalMs() const { return m_writeIntervalMs; }
    int deadbandMillidegrees() const { return m_deadbandMillidegrees; }
    double outputMinPercent() const { return m_outputMinPercent; }
    double outputMaxPercent() const { return m_outputMaxPercent; }

    // --- Q_INVOKABLE methods for DBus operations ---

    Q_INVOKABLE void loadFan(const QString &fanId);
    Q_INVOKABLE void setEnrolledViaDBus(bool enrolled);
    Q_INVOKABLE void setSensorIdsViaDBus(const QStringList &sensorIds);
    Q_INVOKABLE void setAggregationViaDBus(const QString &agg);
    Q_INVOKABLE void setTargetTempCelsiusViaDBus(double celsius);
    Q_INVOKABLE void setPidGains(double kp, double ki, double kd);
    Q_INVOKABLE void setControlModeViaDBus(const QString &mode);
    Q_INVOKABLE void validateDraft();
    Q_INVOKABLE void applyDraft();
    Q_INVOKABLE void discardDraft();
    Q_INVOKABLE void startAutoTune();
    Q_INVOKABLE void acceptAutoTuneProposal();
    Q_INVOKABLE void dismissAutoTuneProposal();

    // Advanced controls setters — send profile updates through daemon
    Q_INVOKABLE void setAdvancedCadence(int sampleMs, int controlMs, int writeMs);
    Q_INVOKABLE void setDeadbandMillidegrees(int millideg);
    Q_INVOKABLE void setOutputRange(double minPercent, double maxPercent);

signals:
    void fanIdChanged();
    void enrolledChanged();
    void controlModeChanged();
    void sensorIdsChanged();
    void aggregationChanged();
    void targetTempCelsiusChanged();
    void pidGainsChanged();
    void autoTuneRunningChanged();
    void autoTuneProposalAvailableChanged();
    void autoTuneProposalChanged();
    void validationStateChanged();
    void applyStateChanged();
    void advancedControlsChanged();

private slots:
    void onDraftConfigResult(const QString &json);
    void onAppliedConfigResult(const QString &json);
    void onWriteSucceeded(const QString &method);
    void onWriteFailed(const QString &method, const QString &error);
    void onAutoTuneResultReady(const QString &fanId, const QString &json);

private:
    void clearValidationState();
    void clearApplyState();
    QJsonObject buildEnrollmentJson() const;
    QJsonObject buildProfileJson() const;
    void parseFanEntry(const QJsonObject &fanObj);
    void parseAppliedForFan(const QString &appliedJson);

    DaemonInterface *m_daemon;

    QString m_fanId;
    bool m_enrolled = false;
    QString m_controlMode;
    QStringList m_sensorIds;
    QString m_aggregation;
    qint64 m_targetTempMillidegrees = 0;
    double m_kp = 1.0;
    double m_ki = 1.0;
    double m_kd = 0.5;

    bool m_autoTuneRunning = false;
    bool m_autoTuneProposalAvailable = false;
    double m_proposedKp = 0.0;
    double m_proposedKi = 0.0;
    double m_proposedKd = 0.0;

    bool m_hasValidationError = false;
    QStringList m_validationErrors;
    bool m_hasApplyError = false;
    QStringList m_applyErrors;

    // Cached config JSONs for reloading
    QString m_cachedDraftJson;
    QString m_cachedAppliedJson;

    // Advanced controls state
    int m_sampleIntervalMs = 1000;
    int m_controlIntervalMs = 2000;
    int m_writeIntervalMs = 2000;
    int m_deadbandMillidegrees = 1000; // 1.0 °C default
    double m_outputMinPercent = 0.0;
    double m_outputMaxPercent = 100.0;
};

#endif // DRAFT_MODEL_H