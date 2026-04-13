/*
 * KDE Fan Control — Draft Editing Model Implementation
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "draft_model.h"
#include "../daemon_interface.h"

#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QDebug>
#include <QProcessEnvironment>

static bool kfcDebug() {
    static bool s = qEnvironmentVariableIsSet("KFC_GUI_DEBUG");
    return s;
}

DraftModel::DraftModel(DaemonInterface *daemon, QObject *parent)
    : QObject(parent)
    , m_daemon(daemon)
{
    // Connect to DaemonInterface signals for reactive model updates.
    connect(m_daemon, &DaemonInterface::draftConfigResult,
            this, &DraftModel::onDraftConfigResult);
    connect(m_daemon, &DaemonInterface::appliedConfigResult,
            this, &DraftModel::onAppliedConfigResult);
    connect(m_daemon, &DaemonInterface::writeSucceeded,
            this, &DraftModel::onWriteSucceeded);
    connect(m_daemon, &DaemonInterface::writeFailed,
            this, &DraftModel::onWriteFailed);
    connect(m_daemon, &DaemonInterface::autoTuneResultReady,
            this, &DraftModel::onAutoTuneResultReady);
}

void DraftModel::setFanId(const QString &id)
{
    if (m_fanId != id) {
        m_fanId = id;
        Q_EMIT fanIdChanged();
    }
}

// --- loadFan: fetches draft and applied config, merges for display ---

void DraftModel::loadFan(const QString &fanId)
{
    if (kfcDebug()) qInfo().noquote() << "KFC_GUI_DEBUG draftModel::loadFan" << fanId;

    // Reset all editable properties to defaults immediately so the UI
    // doesn't show the previous fan's values during the async DBus response.
    setEnrolled(false);
    setControlMode(QStringLiteral("pwm"));
    setSensorIds(QStringList());
    setAggregation(QStringLiteral("average"));
    m_targetTempMillidegrees = 0;
    Q_EMIT targetTempCelsiusChanged();
    m_kp = 1.0; m_ki = 1.0; m_kd = 0.5;
    Q_EMIT pidGainsChanged();
    m_proposedKp = 0; m_proposedKi = 0; m_proposedKd = 0;
    Q_EMIT autoTuneProposalChanged();
    m_sampleIntervalMs = 1000;
    m_controlIntervalMs = 2000;
    m_writeIntervalMs = 2000;
    m_deadbandMillidegrees = 1000;
    m_outputMinPercent = 0.0;
    m_outputMaxPercent = 100.0;
    Q_EMIT advancedControlsChanged();

    setFanId(fanId);
    clearValidationState();
    clearApplyState();
    m_autoTuneProposalAvailable = false;
    m_autoTuneRunning = false;
    Q_EMIT autoTuneProposalAvailableChanged();
    Q_EMIT autoTuneRunningChanged();

    // Request fresh draft and applied config from the daemon.
    m_daemon->draftConfig();
    m_daemon->appliedConfig();
}

// --- Enrollment ---

void DraftModel::setEnrolled(bool enrolled)
{
    if (m_enrolled != enrolled) {
        m_enrolled = enrolled;
        Q_EMIT enrolledChanged();
    }
}

void DraftModel::setEnrolledViaDBus(bool enrolled)
{
    if (kfcDebug()) qInfo().noquote() << "KFC_GUI_DEBUG draftModel::setEnrolledViaDBus" << enrolled;
    setEnrolled(enrolled);
    if (enrolled) {
        m_daemon->setDraftFanEnrollment(
            m_fanId,
            enrolled,
            m_controlMode.isEmpty() ? QStringLiteral("pwm") : m_controlMode,
            m_sensorIds
        );
        if (!m_aggregation.isEmpty() && m_aggregation != QStringLiteral("average")) {
            QJsonObject profileObj;
            profileObj[QStringLiteral("aggregation")] = m_aggregation;
            m_daemon->setDraftFanControlProfile(m_fanId, QJsonDocument(profileObj).toJson(QJsonDocument::Compact));
        }
    } else {
        // Unenrolling: remove the fan from the draft entirely rather than
        // sending a full DraftFanEntry with managed=false, which would
        // wipe the fan's existing settings (target temp, PID gains, etc.).
        // Removing from draft means apply_draft will preserve the fan's
        // previous applied config if one exists.
        m_daemon->removeDraftFan(m_fanId);
    }
}

// --- Control mode ---

void DraftModel::setControlMode(const QString &mode)
{
    if (m_controlMode != mode) {
        m_controlMode = mode;
        Q_EMIT controlModeChanged();
    }
}

void DraftModel::setControlModeViaDBus(const QString &mode)
{
    if (kfcDebug()) qInfo().noquote() << "KFC_GUI_DEBUG draftModel::setControlModeViaDBus" << mode;
    setControlMode(mode);
    m_daemon->setDraftFanEnrollment(
        m_fanId,
        m_enrolled,
        mode,
        m_sensorIds
    );
}

// --- Sensor IDs ---

void DraftModel::setSensorIds(const QStringList &ids)
{
    if (m_sensorIds != ids) {
        m_sensorIds = ids;
        Q_EMIT sensorIdsChanged();
    }
}

void DraftModel::setSensorIdsViaDBus(const QStringList &sensorIds)
{
    if (kfcDebug()) qInfo().noquote() << "KFC_GUI_DEBUG draftModel::setSensorIdsViaDBus" << sensorIds;
    setSensorIds(sensorIds);
    QJsonObject profileObj;
    QJsonArray sourcesArr;
    for (const auto &id : sensorIds) {
        sourcesArr.append(id);
    }
    profileObj[QStringLiteral("temp_sources")] = sourcesArr;
    QJsonDocument doc(profileObj);
    m_daemon->setDraftFanControlProfile(m_fanId, QString::fromUtf8(doc.toJson(QJsonDocument::Compact)));
}

// --- Aggregation ---

void DraftModel::setAggregation(const QString &agg)
{
    if (m_aggregation != agg) {
        m_aggregation = agg;
        Q_EMIT aggregationChanged();
    }
}

void DraftModel::setAggregationViaDBus(const QString &agg)
{
    setAggregation(agg);
    QJsonObject profileObj;
    profileObj[QStringLiteral("aggregation")] = agg;
    QJsonDocument doc(profileObj);
    m_daemon->setDraftFanControlProfile(m_fanId, QString::fromUtf8(doc.toJson(QJsonDocument::Compact)));
}

// --- Target temperature ---

void DraftModel::setTargetTempCelsius(double celsius)
{
    qint64 millidegrees = static_cast<qint64>(celsius * 1000.0);
    if (m_targetTempMillidegrees != millidegrees) {
        m_targetTempMillidegrees = millidegrees;
        Q_EMIT targetTempCelsiusChanged();
    }
}

void DraftModel::setTargetTempCelsiusViaDBus(double celsius)
{
    setTargetTempCelsius(celsius);
    QJsonObject profileObj;
    profileObj[QStringLiteral("target_temp_millidegrees")] = m_targetTempMillidegrees;
    QJsonDocument doc(profileObj);
    m_daemon->setDraftFanControlProfile(m_fanId, QString::fromUtf8(doc.toJson(QJsonDocument::Compact)));
}

// --- PID gains ---

void DraftModel::setPidGains(double kp, double ki, double kd)
{
    if (!qFuzzyCompare(m_kp, kp) || !qFuzzyCompare(m_ki, ki) || !qFuzzyCompare(m_kd, kd)) {
        m_kp = kp;
        m_ki = ki;
        m_kd = kd;
        Q_EMIT pidGainsChanged();
    }
    // Send updated gains via the control profile
    QJsonObject profileObj;
    QJsonObject gainsObj;
    gainsObj[QStringLiteral("kp")] = kp;
    gainsObj[QStringLiteral("ki")] = ki;
    gainsObj[QStringLiteral("kd")] = kd;
    profileObj[QStringLiteral("pid_gains")] = gainsObj;
    QJsonDocument doc(profileObj);
    m_daemon->setDraftFanControlProfile(m_fanId, QString::fromUtf8(doc.toJson(QJsonDocument::Compact)));
}

// --- Validation ---

void DraftModel::validateDraft()
{
    clearValidationState();
    // The validate_draft DBus method is void — we send the call and 
    // results will be returned via draftConfigResult signal after validation.
    // For the GUI we flush local values first by calling draft methods,
    // then validate.
    m_daemon->validateDraft();
}

// --- Apply ---

void DraftModel::applyDraft()
{
    clearApplyState();
    // The apply_draft method on the daemon validates first, then applies.
    // Results come back as JSON via write signal.
    m_daemon->applyDraft();
}

// --- Discard ---

void DraftModel::discardDraft()
{
    clearValidationState();
    clearApplyState();
    m_daemon->discardDraft();
    // Reload fan data after discard
    m_daemon->draftConfig();
    m_daemon->appliedConfig();
}

// --- Auto-tune ---

void DraftModel::startAutoTune()
{
    if (!m_autoTuneRunning) {
        m_autoTuneRunning = true;
        m_autoTuneProposalAvailable = false;
        Q_EMIT autoTuneRunningChanged();
        Q_EMIT autoTuneProposalAvailableChanged();
        m_daemon->startAutoTune(m_fanId);
    }
}

void DraftModel::acceptAutoTuneProposal()
{
    if (m_autoTuneProposalAvailable) {
        // Stage the proposed gains into the local draft
        m_kp = m_proposedKp;
        m_ki = m_proposedKi;
        m_kd = m_proposedKd;
        Q_EMIT pidGainsChanged();

        // Accept via daemon — this stages the auto-tune gains into the draft
        m_daemon->acceptAutoTune(m_fanId);

        // Clear the proposal state
        m_autoTuneProposalAvailable = false;
        Q_EMIT autoTuneProposalAvailableChanged();

        // User must still "Apply changes" to make them live (per D-19)
    }
}

void DraftModel::dismissAutoTuneProposal()
{
    if (m_autoTuneProposalAvailable) {
        m_autoTuneProposalAvailable = false;
        m_proposedKp = 0.0;
        m_proposedKi = 0.0;
        m_proposedKd = 0.0;
        Q_EMIT autoTuneProposalAvailableChanged();
        Q_EMIT autoTuneProposalChanged();
    }
}

void DraftModel::setAdvancedCadence(int sampleMs, int controlMs, int writeMs)
{
    QJsonObject cadenceObj;
    cadenceObj[QStringLiteral("sample_interval_ms")] = sampleMs;
    cadenceObj[QStringLiteral("control_interval_ms")] = controlMs;
    cadenceObj[QStringLiteral("write_interval_ms")] = writeMs;

    QJsonObject profileObj = buildProfileJson();
    profileObj[QStringLiteral("cadence")] = cadenceObj;

    m_daemon->setDraftFanControlProfile(m_fanId, QJsonDocument(profileObj).toJson(QJsonDocument::Compact));
}

void DraftModel::setDeadbandMillidegrees(int millideg)
{
    QJsonObject profileObj = buildProfileJson();
    profileObj[QStringLiteral("deadband_millidegrees")] = millideg;

    m_daemon->setDraftFanControlProfile(m_fanId, QJsonDocument(profileObj).toJson(QJsonDocument::Compact));
}

void DraftModel::setOutputRange(double minPercent, double maxPercent)
{
    QJsonObject policyObj;
    policyObj[QStringLiteral("output_min_percent")] = minPercent;
    policyObj[QStringLiteral("output_max_percent")] = maxPercent;

    QJsonObject profileObj = buildProfileJson();
    profileObj[QStringLiteral("actuator_policy")] = policyObj;

    m_daemon->setDraftFanControlProfile(m_fanId, QJsonDocument(profileObj).toJson(QJsonDocument::Compact));
}

// --- Private helpers ---

void DraftModel::clearValidationState()
{
    m_hasValidationError = false;
    m_validationErrors.clear();
    Q_EMIT validationStateChanged();
}

void DraftModel::clearApplyState()
{
    m_hasApplyError = false;
    m_applyErrors.clear();
    Q_EMIT applyStateChanged();
}

QJsonObject DraftModel::buildEnrollmentJson() const
{
    QJsonObject obj;
    obj[QStringLiteral("managed")] = m_enrolled;
    obj[QStringLiteral("control_mode")] = m_controlMode.isEmpty()
        ? QStringLiteral("pwm") : m_controlMode;
    return obj;
}

QJsonObject DraftModel::buildProfileJson() const
{
    QJsonObject obj;
    QJsonArray sourcesArr;
    for (const auto &id : m_sensorIds) {
        sourcesArr.append(id);
    }
    obj[QStringLiteral("temp_sources")] = sourcesArr;
    obj[QStringLiteral("target_temp_millidegrees")] = m_targetTempMillidegrees;
    obj[QStringLiteral("aggregation")] = m_aggregation.isEmpty()
        ? QStringLiteral("average") : m_aggregation;

    QJsonObject gainsObj;
    gainsObj[QStringLiteral("kp")] = m_kp;
    gainsObj[QStringLiteral("ki")] = m_ki;
    gainsObj[QStringLiteral("kd")] = m_kd;
    obj[QStringLiteral("pid_gains")] = gainsObj;

    return obj;
}

// --- Signal handlers for DaemonInterface responses ---

void DraftModel::onDraftConfigResult(const QString &json)
{
    if (kfcDebug()) qInfo().noquote() << "KFC_GUI_DEBUG draftModel::onDraftConfigResult fanId=" << m_fanId << "len=" << json.length();
    m_cachedDraftJson = json;
    if (m_fanId.isEmpty()) return;

    QJsonParseError err;
    QJsonObject root = QJsonDocument::fromJson(json.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError && !json.isEmpty()) {
        qWarning() << "DraftModel: draft config JSON parse error:" << err.errorString();
        return;
    }

    // Find the entry for our current fanId in the draft fans map
    QJsonObject fansObj = root.value(QStringLiteral("fans")).toObject();
    if (!fansObj.contains(m_fanId)) {
        // No draft entry for this fan — use applied config or defaults
        if (!m_cachedAppliedJson.isEmpty()) {
            parseAppliedForFan(m_cachedAppliedJson);
        } else {
            // Reset to defaults
            setEnrolled(false);
            setControlMode(QStringLiteral("pwm"));
            setSensorIds(QStringList());
            setAggregation(QStringLiteral("average"));
            m_targetTempMillidegrees = 0;
            Q_EMIT targetTempCelsiusChanged();
            m_kp = 1.0; m_ki = 1.0; m_kd = 0.5;
            Q_EMIT pidGainsChanged();
        }
        return;
    }

    QJsonObject fanObj = fansObj.value(m_fanId).toObject();
    parseFanEntry(fanObj);
}

void DraftModel::onAppliedConfigResult(const QString &json)
{
    m_cachedAppliedJson = json;
    if (m_fanId.isEmpty()) return;

    // Only parse if there's no draft entry — the draft takes priority.
    // But we still cache it so we can fall back.
    QJsonParseError err;
    QJsonObject root = QJsonDocument::fromJson(json.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError && !json.isEmpty()) {
        return;
    }

    // Merge: if there's a draft entry, draft takes priority. Otherwise, use applied.
    QJsonObject draftRoot = QJsonDocument::fromJson(m_cachedDraftJson.toUtf8(), &err).object();
    QJsonObject draftFans = draftRoot.value(QStringLiteral("fans")).toObject();

    if (draftFans.contains(m_fanId)) {
        // Draft entry exists — use draft values
        QJsonObject fanObj = draftFans.value(m_fanId).toObject();
        parseFanEntry(fanObj);
    } else {
        // No draft — use applied config
        parseAppliedForFan(json);
    }
}

void DraftModel::parseFanEntry(const QJsonObject &fanObj)
{
    setEnrolled(fanObj.value(QStringLiteral("managed")).toBool(false));

    QString mode = fanObj.value(QStringLiteral("control_mode")).toString(QStringLiteral("pwm"));
    setControlMode(mode);

    QJsonArray sourcesArr = fanObj.value(QStringLiteral("temp_sources")).toArray();
    QStringList sources;
    for (const auto &v : sourcesArr) {
        sources.append(v.toString());
    }
    setSensorIds(sources);

    QString agg = fanObj.value(QStringLiteral("aggregation")).toString(QStringLiteral("average"));
    if (agg.isEmpty()) agg = QStringLiteral("average");
    setAggregation(agg);

    qint64 targetTemp = fanObj.value(QStringLiteral("target_temp_millidegrees")).toVariant().toLongLong();
    if (m_targetTempMillidegrees != targetTemp) {
        m_targetTempMillidegrees = targetTemp;
        Q_EMIT targetTempCelsiusChanged();
    }

    QJsonObject gainsObj = fanObj.value(QStringLiteral("pid_gains")).toObject();
    double kp = gainsObj.value(QStringLiteral("kp")).toDouble(1.0);
    double ki = gainsObj.value(QStringLiteral("ki")).toDouble(0.1);
    double kd = gainsObj.value(QStringLiteral("kd")).toDouble(0.5);
    if (!qFuzzyCompare(m_kp, kp) || !qFuzzyCompare(m_ki, ki) || !qFuzzyCompare(m_kd, kd)) {
        m_kp = kp;
        m_ki = ki;
        m_kd = kd;
        Q_EMIT pidGainsChanged();
    }

    // Parse cadence
    QJsonObject cadenceObj = fanObj.value(QStringLiteral("cadence")).toObject();
    int sampleMs = static_cast<int>(cadenceObj.value(QStringLiteral("sample_interval_ms")).toVariant().toLongLong());
    int controlMs = static_cast<int>(cadenceObj.value(QStringLiteral("control_interval_ms")).toVariant().toLongLong());
    int writeMs = static_cast<int>(cadenceObj.value(QStringLiteral("write_interval_ms")).toVariant().toLongLong());

    // Parse deadband
    qint64 deadband = fanObj.value(QStringLiteral("deadband_millidegrees")).toVariant().toLongLong();

    // Parse actuator policy
    QJsonObject policyObj = fanObj.value(QStringLiteral("actuator_policy")).toObject();
    double outMin = policyObj.value(QStringLiteral("output_min_percent")).toDouble(0.0);
    double outMax = policyObj.value(QStringLiteral("output_max_percent")).toDouble(100.0);

    bool advancedChanged = false;
    if (m_sampleIntervalMs != sampleMs) { m_sampleIntervalMs = sampleMs; advancedChanged = true; }
    if (m_controlIntervalMs != controlMs) { m_controlIntervalMs = controlMs; advancedChanged = true; }
    if (m_writeIntervalMs != writeMs) { m_writeIntervalMs = writeMs; advancedChanged = true; }
    if (m_deadbandMillidegrees != static_cast<int>(deadband)) { m_deadbandMillidegrees = static_cast<int>(deadband); advancedChanged = true; }
    if (!qFuzzyCompare(m_outputMinPercent, outMin)) { m_outputMinPercent = outMin; advancedChanged = true; }
    if (!qFuzzyCompare(m_outputMaxPercent, outMax)) { m_outputMaxPercent = outMax; advancedChanged = true; }
    if (advancedChanged) {
        Q_EMIT advancedControlsChanged();
    }
}

void DraftModel::parseAppliedForFan(const QString &appliedJson)
{
    QJsonParseError err;
    QJsonObject root = QJsonDocument::fromJson(appliedJson.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError && !appliedJson.isEmpty()) {
        return;
    }

    QJsonObject fansObj = root.value(QStringLiteral("fans")).toObject();
    if (!fansObj.contains(m_fanId)) {
        // No applied entry — use defaults
        setEnrolled(false);
        return;
    }

    QJsonObject fanObj = fansObj.value(m_fanId).toObject();
    // Applied config always means enrolled
    setEnrolled(true);
    parseFanEntry(fanObj);
}

void DraftModel::onWriteSucceeded(const QString &method)
{
    if (kfcDebug()) qInfo().noquote() << "KFC_GUI_DEBUG draftModel::onWriteSucceeded" << method;
    // After write operations that change state, refresh the models.
    if (method == QStringLiteral("validateDraft")) {
        // Validation succeeded — the daemon returned success.
        // We'll get validation details via the draft config update.
        m_daemon->draftConfig();
    } else if (method == QStringLiteral("applyDraft")) {
        m_daemon->runtimeState();
        m_daemon->draftConfig();
        m_daemon->appliedConfig();
    } else if (method == QStringLiteral("setDraftFanEnrollment")
               || method == QStringLiteral("setDraftFanControlProfile")) {
        m_daemon->draftConfig();
    } else if (method == QStringLiteral("acceptAutoTune")) {
        m_daemon->draftConfig();
    }
}

void DraftModel::onWriteFailed(const QString &method, const QString &error)
{
    if (kfcDebug()) qInfo().noquote() << "KFC_GUI_DEBUG draftModel::onWriteFailed" << method << error;
    qWarning() << "DraftModel: write failed for" << method << ":" << error;

    // Surface authorization errors prominently per T-04-05
    if (error.contains(QStringLiteral("authorized"), Qt::CaseInsensitive)
        || error.contains(QStringLiteral("permission"), Qt::CaseInsensitive)
        || error.contains(QStringLiteral("AccessDenied"), Qt::CaseInsensitive)) {
        m_hasApplyError = true;
        m_applyErrors = QStringList{
            QStringLiteral("This action requires elevated privileges. Run the GUI with the required authorization and try again.")
        };
        Q_EMIT applyStateChanged();
    } else if (method == QStringLiteral("validateDraft")) {
        m_hasValidationError = true;
        m_validationErrors = QStringList{ error };
        Q_EMIT validationStateChanged();
    } else if (method == QStringLiteral("applyDraft")) {
        m_hasApplyError = true;
        m_applyErrors = QStringList{ error };
        Q_EMIT applyStateChanged();
    }
}

void DraftModel::onAutoTuneResultReady(const QString &fanId, const QString &json)
{
    // Only process auto-tune results for our current fan
    if (fanId != m_fanId) return;

    m_autoTuneRunning = false;
    Q_EMIT autoTuneRunningChanged();

    QJsonParseError err;
    QJsonObject result = QJsonDocument::fromJson(json.toUtf8(), &err).object();
    if (err.error != QJsonParseError::NoError) {
        qWarning() << "DraftModel: auto-tune result JSON parse error:" << err.errorString();
        return;
    }

    QString status = result.value(QStringLiteral("status")).toString();
    if (status == QStringLiteral("completed")) {
        QJsonObject proposalObj = result.value(QStringLiteral("proposal")).toObject();
        QJsonObject gainsObj = proposalObj.value(QStringLiteral("proposed_gains")).toObject();
        m_proposedKp = gainsObj.value(QStringLiteral("kp")).toDouble(1.0);
        m_proposedKi = gainsObj.value(QStringLiteral("ki")).toDouble(0.1);
        m_proposedKd = gainsObj.value(QStringLiteral("kd")).toDouble(0.5);
        m_autoTuneProposalAvailable = true;
        Q_EMIT autoTuneProposalAvailableChanged();
        Q_EMIT autoTuneProposalChanged();
    } else if (status == QStringLiteral("failed")) {
        // Auto-tune failed — surface as an apply error
        m_hasApplyError = true;
        QString failError = result.value(QStringLiteral("error")).toString(
            QStringLiteral("Auto-tune could not complete. Check sensor availability and current runtime state, then try again."));
        m_applyErrors = QStringList{ failError };
        Q_EMIT applyStateChanged();
    }
    // "idle" or "running" — no action needed beyond clearing autoTuneRunning
}