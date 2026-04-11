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
    double m_ki = 0.1;
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
};

#endif // DRAFT_MODEL_H