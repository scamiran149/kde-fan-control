use clap::Parser;
use kde_fan_control_daemon::args::DaemonArgs;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = DaemonArgs::parse();
    kde_fan_control_daemon::app::startup::run(args).await
}

#[cfg(test)]
mod tests {
    use kde_fan_control_core::config::LifecycleEventLog;
    use kde_fan_control_core::config::{AppConfig, AppliedConfig, AppliedFanEntry, DegradedState};
    use kde_fan_control_core::control::{
        ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits,
    };
    use kde_fan_control_core::inventory::{
        ControlMode, HwmonDevice, InventorySnapshot, TemperatureSensor,
    };
    use kde_fan_control_core::lifecycle::OwnedFanSet;
    use kde_fan_control_daemon::safety::fallback::record_fallback_incident_for_owned;
    use kde_fan_control_daemon::safety::panic_hook::run_panic_fallback_recorder;
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::sync::RwLock;

    use kde_fan_control_core::config::DegradedReason;
    use kde_fan_control_daemon::control::supervisor::ControlSupervisor;
    use kde_fan_control_daemon::dbus::control::ControlIface;
    use kde_fan_control_daemon::state::AutoTuneResultView;

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn applied_entry(temp_sources: Vec<String>) -> AppliedFanEntry {
        AppliedFanEntry {
            control_mode: ControlMode::Pwm,
            temp_sources,
            target_temp_millidegrees: 50_000,
            aggregation: AggregationFn::Average,
            pid_gains: PidGains {
                kp: 1.0,
                ki: 0.0,
                kd: 0.0,
            },
            cadence: ControlCadence {
                sample_interval_ms: 20,
                control_interval_ms: 20,
                write_interval_ms: 20,
            },
            deadband_millidegrees: 0,
            actuator_policy: ActuatorPolicy {
                output_min_percent: 0.0,
                output_max_percent: 100.0,
                pwm_min: 0,
                pwm_max: 255,
                startup_kick_percent: 35.0,
                startup_kick_ms: 1,
            },
            pid_limits: PidLimits::default(),
        }
    }

    fn applied_config_for(fan_id: &str, temp_id: &str) -> AppliedConfig {
        AppliedConfig {
            fans: HashMap::from([(fan_id.to_string(), applied_entry(vec![temp_id.to_string()]))]),
            applied_at: Some("2026-04-11T12:00:00Z".to_string()),
        }
    }

    fn test_snapshot(root: &Path) -> InventorySnapshot {
        InventorySnapshot {
            devices: vec![HwmonDevice {
                id: "hwmon-test-0000000000000001".to_string(),
                name: "testchip".to_string(),
                sysfs_path: root.display().to_string(),
                stable_identity: "/sys/devices/platform/testchip".to_string(),
                temperatures: vec![TemperatureSensor {
                    id: "hwmon-test-0000000000000001-temp1".to_string(),
                    channel: 1,
                    label: Some("CPU".to_string()),
                    friendly_name: None,
                    input_millidegrees_celsius: Some(55_000),
                }],
                fans: vec![kde_fan_control_core::inventory::FanChannel {
                    id: "hwmon-test-0000000000000001-fan1".to_string(),
                    channel: 1,
                    label: Some("CPU Fan".to_string()),
                    friendly_name: None,
                    rpm_feedback: true,
                    current_rpm: Some(1200),
                    control_modes: vec![ControlMode::Pwm],
                    support_state: kde_fan_control_core::inventory::SupportState::Available,
                    support_reason: None,
                }],
            }],
        }
    }

    struct ControlFixture {
        root: PathBuf,
    }

    impl ControlFixture {
        fn new() -> Self {
            let unique = format!(
                "kde-fan-control-daemon-control-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock should be after epoch")
                    .as_nanos(),
                TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let root = std::env::temp_dir().join(unique);
            fs::create_dir_all(&root).expect("fixture root should be created");
            Self { root }
        }

        fn root(&self) -> &Path {
            &self.root
        }

        fn write_temp(&self, value: &str) {
            fs::write(self.root.join("temp1_input"), value).expect("temp input should be written");
        }

        fn write_pwm_seed(&self, value: &str) {
            fs::write(self.root.join("pwm1"), value).expect("pwm file should be written");
        }

        fn pwm_path(&self) -> PathBuf {
            self.root.join("pwm1")
        }
    }

    impl Drop for ControlFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn shared_fallback_recorder_persists_incident_for_graceful_shutdown() {
        let mut owned = OwnedFanSet::new();
        owned.claim_fan("fan-1", ControlMode::Pwm, "/definitely/missing/pwm1");
        let mut config = AppConfig::default();
        let mut events = LifecycleEventLog::new();
        let mut fallback_fan_ids = HashSet::new();

        let result = record_fallback_incident_for_owned(
            &owned,
            &mut config,
            &mut events,
            &mut fallback_fan_ids,
            "ctrl-c shutdown".to_string(),
        );

        assert_eq!(result.failed.len(), 1);
        let incident = config
            .fallback_incident
            .as_ref()
            .expect("fallback incident");
        assert_eq!(incident.affected_fans, vec!["fan-1"]);
        assert!(fallback_fan_ids.contains("fan-1"));
        assert!(matches!(
            events.events().last().map(|event| &event.reason),
            Some(DegradedReason::FallbackActive { affected_fans }) if affected_fans == &vec!["fan-1".to_string()]
        ));
    }

    #[test]
    fn panic_path_uses_same_fallback_recorder() {
        let mut owned = OwnedFanSet::new();
        owned.claim_fan("fan-1", ControlMode::Pwm, "/definitely/missing/pwm1");
        let owned = Arc::new(RwLock::new(owned));
        let config = Arc::new(RwLock::new(AppConfig::default()));
        let events = Arc::new(RwLock::new(LifecycleEventLog::new()));
        let fallback_fan_ids = Arc::new(RwLock::new(HashSet::new()));

        assert!(run_panic_fallback_recorder(
            &owned,
            &config,
            &events,
            &fallback_fan_ids,
            "panic: simulated".to_string(),
        ));

        let config = config
            .try_read()
            .expect("config lock should be available after panic recorder completes");
        assert!(config.fallback_incident.is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_supervisor_runs_managed_fan_loops_and_writes_pwm() {
        let fixture = ControlFixture::new();
        fixture.write_temp("55000\n");
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let applied = applied_config_for(
            "hwmon-test-0000000000000001-fan1",
            "hwmon-test-0000000000000001-temp1",
        );
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied),
            ..AppConfig::default()
        }));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        let degraded = Arc::new(RwLock::new(DegradedState::new()));

        let supervisor = ControlSupervisor::new(snapshot, config, owned, degraded);
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;

        let status = supervisor
            .status_json()
            .await
            .expect("status should serialize");
        assert!(status.contains("hwmon-test-0000000000000001-fan1"));
        assert!(status.contains("logical_output_percent"));

        let pwm = fs::read_to_string(fixture.pwm_path()).expect("pwm should be readable");
        assert_ne!(pwm.trim(), "0");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_supervisor_degrades_when_all_temp_sources_fail() {
        let fixture = ControlFixture::new();
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let applied = applied_config_for(
            "hwmon-test-0000000000000001-fan1",
            "hwmon-test-0000000000000001-temp1",
        );
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied),
            ..AppConfig::default()
        }));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        let degraded = Arc::new(RwLock::new(DegradedState::new()));

        let supervisor = ControlSupervisor::new(snapshot, config, owned, Arc::clone(&degraded));
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;

        let degraded = degraded.read().await;
        let reasons = degraded
            .entries
            .get("hwmon-test-0000000000000001-fan1")
            .expect("fan should be degraded");
        assert!(matches!(
            reasons.first(),
            Some(DegradedReason::TempSourceMissing { .. })
        ));

        let status = supervisor
            .status_json()
            .await
            .expect("status should serialize");
        assert!(!status.contains("hwmon-test-0000000000000001-fan1"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_supervisor_skips_unowned_fans_and_stops_after_ownership_loss() {
        let fixture = ControlFixture::new();
        fixture.write_temp("57000\n");
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let applied = applied_config_for(
            "hwmon-test-0000000000000001-fan1",
            "hwmon-test-0000000000000001-temp1",
        );
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied),
            ..AppConfig::default()
        }));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        let degraded = Arc::new(RwLock::new(DegradedState::new()));

        let supervisor = ControlSupervisor::new(
            Arc::clone(&snapshot),
            Arc::clone(&config),
            Arc::clone(&owned),
            degraded,
        );
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(
            fs::read_to_string(fixture.pwm_path())
                .expect("pwm should be readable")
                .trim(),
            "0"
        );

        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        let written = fs::read_to_string(fixture.pwm_path()).expect("pwm should be readable");
        assert_ne!(written.trim(), "0");

        owned
            .write()
            .await
            .release_fan("hwmon-test-0000000000000001-fan1");
        tokio::time::sleep(Duration::from_millis(80)).await;
        let after_release = fs::read_to_string(fixture.pwm_path()).expect("pwm should be readable");
        tokio::time::sleep(Duration::from_millis(80)).await;
        let final_pwm = fs::read_to_string(fixture.pwm_path()).expect("pwm should be readable");
        assert_eq!(after_release, final_pwm);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_iface_get_control_status_serializes_live_snapshots() {
        let fixture = ControlFixture::new();
        fixture.write_temp("56000\n");
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied_config_for(
                "hwmon-test-0000000000000001-fan1",
                "hwmon-test-0000000000000001-temp1",
            )),
            ..AppConfig::default()
        }));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        let degraded = Arc::new(RwLock::new(DegradedState::new()));
        let supervisor = ControlSupervisor::new(snapshot, config, owned, degraded);
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;

        let iface = ControlIface {
            supervisor,
            config: Arc::new(RwLock::new(AppConfig::default())),
        };
        let status = iface
            .get_control_status()
            .await
            .expect("control status should serialize");
        assert!(status.contains("hwmon-test-0000000000000001-fan1"));
        assert!(status.contains("aggregated_temp_millidegrees"));
        assert!(status.contains("mapped_pwm"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_supervisor_reconciles_after_applied_config_changes() {
        let fixture = ControlFixture::new();
        fixture.write_temp("56500\n");
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let config = Arc::new(RwLock::new(AppConfig::default()));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        let degraded = Arc::new(RwLock::new(DegradedState::new()));
        let supervisor = ControlSupervisor::new(
            Arc::clone(&snapshot),
            Arc::clone(&config),
            Arc::clone(&owned),
            degraded,
        );

        supervisor.reconcile().await;
        assert_eq!(supervisor.status_json().await.expect("status"), "{}");

        {
            let mut config = config.write().await;
            config.applied = Some(applied_config_for(
                "hwmon-test-0000000000000001-fan1",
                "hwmon-test-0000000000000001-temp1",
            ));
        }
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        let started = supervisor.status_json().await.expect("status");
        assert!(started.contains("hwmon-test-0000000000000001-fan1"));

        {
            let mut config = config.write().await;
            config.applied = Some(AppliedConfig {
                fans: HashMap::new(),
                applied_at: Some("2026-04-11T12:05:00Z".to_string()),
            });
        }
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(supervisor.status_json().await.expect("status"), "{}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_supervisor_degrades_on_pwm_write_failure_keeps_owned() {
        let fixture = ControlFixture::new();
        fixture.write_temp("56000\n");
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied_config_for(
                "hwmon-test-0000000000000001-fan1",
                "hwmon-test-0000000000000001-temp1",
            )),
            ..AppConfig::default()
        }));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        let degraded = Arc::new(RwLock::new(DegradedState::new()));

        let supervisor = ControlSupervisor::new(
            Arc::clone(&snapshot),
            Arc::clone(&config),
            Arc::clone(&owned),
            Arc::clone(&degraded),
        );
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(40)).await;

        fs::remove_file(fixture.pwm_path()).expect("should remove pwm file to force write failure");
        fs::create_dir(fixture.pwm_path())
            .expect("should replace pwm file with directory to force write failure");
        tokio::time::sleep(Duration::from_millis(80)).await;

        assert!(owned.read().await.owns("hwmon-test-0000000000000001-fan1"));
        let degraded = degraded.read().await;
        assert!(
            degraded
                .entries
                .contains_key("hwmon-test-0000000000000001-fan1")
        );
        let status = supervisor
            .status_json()
            .await
            .expect("status should serialize");
        assert!(!status.contains("hwmon-test-0000000000000001-fan1"));
    }

    #[test]
    fn release_removed_owned_fans_drops_fans_not_in_next_applied_set() {
        let fixture = ControlFixture::new();
        fixture.write_pwm_seed("0\n");

        let mut owned = OwnedFanSet::new();
        owned.claim_fan(
            "fan-a",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        owned.claim_fan("fan-b", ControlMode::Pwm, "/sys/class/hwmon/hwmon0/pwm2");

        let next_owned = HashSet::from(["fan-b".to_string()]);
        let failures = kde_fan_control_daemon::dbus::lifecycle_apply::release_removed_owned_fans(
            &mut owned,
            &next_owned,
        );

        assert!(failures.is_empty());
        assert!(!owned.owns("fan-a"));
        assert!(owned.owns("fan-b"));
    }

    #[test]
    fn release_removed_owned_fans_keeps_ownership_on_fallback_failure() {
        let mut owned = OwnedFanSet::new();
        owned.claim_fan("fan-a", ControlMode::Pwm, "/definitely/missing/pwm1");

        let next_owned = HashSet::new();
        let failures = kde_fan_control_daemon::dbus::lifecycle_apply::release_removed_owned_fans(
            &mut owned,
            &next_owned,
        );

        assert_eq!(failures.len(), 1);
        assert!(owned.owns("fan-a"));
    }

    async fn auto_tune_test_harness(
        fixture: &ControlFixture,
    ) -> (
        ControlSupervisor,
        Arc<RwLock<AppConfig>>,
        Arc<RwLock<DegradedState>>,
    ) {
        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let applied = applied_config_for(
            "hwmon-test-0000000000000001-fan1",
            "hwmon-test-0000000000000001-temp1",
        );
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied),
            ..AppConfig::default()
        }));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        let degraded = Arc::new(RwLock::new(DegradedState::new()));
        let supervisor = ControlSupervisor::new(
            Arc::clone(&snapshot),
            Arc::clone(&config),
            owned,
            Arc::clone(&degraded),
        );
        supervisor.set_auto_tune_observation_window_ms(40).await;
        supervisor.reconcile().await;
        (supervisor, config, degraded)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auto_tune_start_puts_managed_fan_into_bounded_running_state() {
        let fixture = ControlFixture::new();
        fixture.write_temp("60000\n");
        fixture.write_pwm_seed("0\n");

        let (supervisor, _, _) = auto_tune_test_harness(&fixture).await;
        supervisor
            .start_auto_tune("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune should start");

        let result = supervisor
            .auto_tune_result_json("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune result should serialize");
        assert!(result.contains("running"));
        assert!(result.contains("40"));

        let status = supervisor
            .status_json()
            .await
            .expect("status should serialize");
        assert!(status.contains("\"auto_tuning\":true"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auto_tune_unreadable_temp_records_failure_without_mutating_applied_gains() {
        let fixture = ControlFixture::new();
        fixture.write_pwm_seed("0\n");

        let (supervisor, config, _) = auto_tune_test_harness(&fixture).await;
        let original_gains = config
            .read()
            .await
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
            .expect("applied entry should exist")
            .pid_gains;

        supervisor
            .start_auto_tune("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune should start");
        tokio::time::sleep(Duration::from_millis(60)).await;

        let result = supervisor
            .auto_tune_result_json("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune result should serialize");
        assert!(result.contains("failed"));

        let applied_gains = config
            .read()
            .await
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
            .expect("applied entry should exist")
            .pid_gains;
        assert_eq!(applied_gains, original_gains);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auto_tune_completion_exposes_softened_proposal_without_mutating_applied_gains() {
        let fixture = ControlFixture::new();
        fixture.write_temp("60000\n");
        fixture.write_pwm_seed("0\n");

        let (supervisor, config, _) = auto_tune_test_harness(&fixture).await;
        let original_gains = config
            .read()
            .await
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
            .expect("applied entry should exist")
            .pid_gains;

        supervisor
            .start_auto_tune("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune should start");

        tokio::time::sleep(Duration::from_millis(15)).await;
        fixture.write_temp("59000\n");
        tokio::time::sleep(Duration::from_millis(15)).await;
        fixture.write_temp("57500\n");
        tokio::time::sleep(Duration::from_millis(60)).await;

        use kde_fan_control_daemon::state::AutoTuneResultView;
        let result = supervisor
            .auto_tune_result_view("hwmon-test-0000000000000001-fan1")
            .await;
        match result {
            AutoTuneResultView::Completed {
                proposal,
                observation_window_ms,
            } => {
                assert_eq!(observation_window_ms, 40);
                assert!(proposal.proposed_gains.kp > 0.0);
                assert!(proposal.proposed_gains.ki > 0.0);
                assert!(proposal.proposed_gains.kd > 0.0);
            }
            other => panic!("expected completed auto-tune result, got {other:?}"),
        }

        let applied_gains = config
            .read()
            .await
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
            .expect("applied entry should exist")
            .pid_gains;
        assert_eq!(applied_gains, original_gains);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_iface_accept_auto_tune_stages_proposed_gains_into_draft() {
        let fixture = ControlFixture::new();
        fixture.write_temp("60000\n");
        fixture.write_pwm_seed("0\n");

        let (supervisor, config, _) = auto_tune_test_harness(&fixture).await;
        supervisor
            .start_auto_tune("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune should start");
        tokio::time::sleep(Duration::from_millis(15)).await;
        fixture.write_temp("59000\n");
        tokio::time::sleep(Duration::from_millis(15)).await;
        fixture.write_temp("57500\n");
        tokio::time::sleep(Duration::from_millis(60)).await;

        let applied_gains = config
            .read()
            .await
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
            .expect("applied entry should exist")
            .pid_gains;

        let iface = ControlIface {
            supervisor,
            config: Arc::clone(&config),
        };
        let updated = iface
            .accept_auto_tune_for_test("hwmon-test-0000000000000001-fan1", true)
            .await
            .expect("accepted gains should stage into draft");
        assert!(updated.contains("pid_gains"));

        let config_guard = config.read().await;
        let draft_entry = config_guard
            .draft
            .fans
            .get("hwmon-test-0000000000000001-fan1")
            .expect("draft entry should exist");
        assert!(draft_entry.pid_gains.is_some());
        assert_ne!(draft_entry.pid_gains.expect("gains"), applied_gains);
        assert_eq!(
            config_guard
                .applied
                .as_ref()
                .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
                .expect("applied entry should exist")
                .pid_gains,
            applied_gains
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_iface_profile_mutations_enforce_authorization_and_stage_draft_updates() {
        let fixture = ControlFixture::new();
        fixture.write_temp("60000\n");
        fixture.write_pwm_seed("0\n");

        let (supervisor, config, _) = auto_tune_test_harness(&fixture).await;
        let iface = ControlIface {
            supervisor,
            config: Arc::clone(&config),
        };

        use zbus::fdo;
        let unauthorized_accept = iface
            .accept_auto_tune_for_test("hwmon-test-0000000000000001-fan1", false)
            .await;
        assert!(matches!(
            unauthorized_accept,
            Err(fdo::Error::AccessDenied(_))
        ));

        let profile_json = serde_json::json!({
            "target_temp_millidegrees": 68000,
            "aggregation": "max",
            "pid_gains": { "kp": 2.5, "ki": 0.3, "kd": 0.9 },
            "cadence": {
                "sample_interval_ms": 500,
                "control_interval_ms": 1000,
                "write_interval_ms": 1500
            },
            "deadband_millidegrees": 2000,
            "actuator_policy": {
                "output_min_percent": 10.0,
                "output_max_percent": 95.0,
                "pwm_min": 15,
                "pwm_max": 240,
                "startup_kick_percent": 45.0,
                "startup_kick_ms": 1200
            },
            "pid_limits": {
                "integral_min": -20.0,
                "integral_max": 20.0,
                "derivative_min": -6.0,
                "derivative_max": 6.0
            }
        })
        .to_string();

        let unauthorized_profile = iface
            .set_draft_fan_control_profile_for_test(
                "hwmon-test-0000000000000001-fan1",
                &profile_json,
                false,
            )
            .await;
        assert!(matches!(
            unauthorized_profile,
            Err(fdo::Error::AccessDenied(_))
        ));

        let updated = iface
            .set_draft_fan_control_profile_for_test(
                "hwmon-test-0000000000000001-fan1",
                &profile_json,
                true,
            )
            .await
            .expect("authorized profile update should succeed");
        assert!(updated.contains("68000"));

        let config_guard = config.read().await;
        let draft_entry = config_guard
            .draft
            .fans
            .get("hwmon-test-0000000000000001-fan1")
            .expect("draft entry should exist");
        assert_eq!(draft_entry.target_temp_millidegrees, Some(68_000));
        assert_eq!(draft_entry.aggregation, Some(AggregationFn::Max));
        assert_eq!(draft_entry.pid_gains.expect("pid gains").kp, 2.5);
        assert_eq!(
            draft_entry.cadence.expect("cadence").write_interval_ms,
            1_500
        );
        assert_eq!(
            draft_entry
                .actuator_policy
                .expect("actuator policy")
                .startup_kick_percent,
            45.0
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_iface_partial_profile_updates_preserve_unspecified_draft_fields() {
        let fixture = ControlFixture::new();
        fixture.write_temp("60000\n");
        fixture.write_pwm_seed("0\n");

        let (supervisor, config, _) = auto_tune_test_harness(&fixture).await;
        let iface = ControlIface {
            supervisor,
            config: Arc::clone(&config),
        };

        iface
            .set_draft_fan_control_profile_for_test(
                "hwmon-test-0000000000000001-fan1",
                &serde_json::json!({
                    "target_temp_millidegrees": 68000,
                    "aggregation": "max",
                    "pid_gains": { "kp": 2.5, "ki": 0.3, "kd": 0.9 }
                })
                .to_string(),
                true,
            )
            .await
            .expect("seed profile update should succeed");

        iface
            .set_draft_fan_control_profile_for_test(
                "hwmon-test-0000000000000001-fan1",
                &serde_json::json!({
                    "deadband_millidegrees": 3500
                })
                .to_string(),
                true,
            )
            .await
            .expect("partial profile update should succeed");

        let config_guard = config.read().await;
        let draft_entry = config_guard
            .draft
            .fans
            .get("hwmon-test-0000000000000001-fan1")
            .expect("draft entry should exist");
        assert_eq!(draft_entry.target_temp_millidegrees, Some(68_000));
        assert_eq!(draft_entry.aggregation, Some(AggregationFn::Max));
        assert_eq!(draft_entry.pid_gains.expect("pid gains").kp, 2.5);
        assert_eq!(draft_entry.deadband_millidegrees, Some(3_500));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn degrade_and_stop_writes_fallback_pwm() {
        let fixture = ControlFixture::new();
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied_config_for(
                "hwmon-test-0000000000000001-fan1",
                "hwmon-test-0000000000000001-temp1",
            )),
            ..AppConfig::default()
        }));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        let degraded = Arc::new(RwLock::new(DegradedState::new()));

        let supervisor = ControlSupervisor::new(
            Arc::clone(&snapshot),
            Arc::clone(&config),
            Arc::clone(&owned),
            Arc::clone(&degraded),
        );

        supervisor
            .degrade_and_stop(
                "hwmon-test-0000000000000001-fan1",
                DegradedReason::TempSourceMissing {
                    fan_id: "hwmon-test-0000000000000001-fan1".to_string(),
                    temp_id: "hwmon-test-0000000000000001-temp1".to_string(),
                },
            )
            .await;

        let pwm = fs::read_to_string(fixture.pwm_path()).expect("pwm should be readable");
        assert_eq!(pwm.trim(), "255");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn degrade_and_stop_keeps_fan_owned() {
        let fixture = ControlFixture::new();
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied_config_for(
                "hwmon-test-0000000000000001-fan1",
                "hwmon-test-0000000000000001-temp1",
            )),
            ..AppConfig::default()
        }));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        let degraded = Arc::new(RwLock::new(DegradedState::new()));

        let supervisor = ControlSupervisor::new(
            Arc::clone(&snapshot),
            Arc::clone(&config),
            Arc::clone(&owned),
            Arc::clone(&degraded),
        );

        supervisor
            .degrade_and_stop(
                "hwmon-test-0000000000000001-fan1",
                DegradedReason::TempSourceMissing {
                    fan_id: "hwmon-test-0000000000000001-fan1".to_string(),
                    temp_id: "hwmon-test-0000000000000001-temp1".to_string(),
                },
            )
            .await;

        assert!(owned.read().await.owns("hwmon-test-0000000000000001-fan1"));
    }
}
