use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationFn {
    Average,
    Max,
    Min,
    Median,
}

impl Default for AggregationFn {
    fn default() -> Self {
        Self::Average
    }
}

impl AggregationFn {
    pub fn compute_millidegrees(&self, readings: &[i64]) -> Option<i64> {
        if readings.is_empty() {
            return None;
        }

        match self {
            Self::Average => Some(readings.iter().sum::<i64>() / readings.len() as i64),
            Self::Max => readings.iter().copied().max(),
            Self::Min => readings.iter().copied().min(),
            Self::Median => {
                let mut sorted = readings.to_vec();
                sorted.sort_unstable();
                let mid = sorted.len() / 2;
                if sorted.len() % 2 == 0 {
                    Some((sorted[mid - 1] + sorted[mid]) / 2)
                } else {
                    Some(sorted[mid])
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PidGains {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
}

impl Default for PidGains {
    fn default() -> Self {
        Self {
            kp: 1.0,
            ki: 0.1,
            kd: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlCadence {
    pub sample_interval_ms: u64,
    pub control_interval_ms: u64,
    pub write_interval_ms: u64,
}

impl Default for ControlCadence {
    fn default() -> Self {
        Self {
            sample_interval_ms: 1_000,
            control_interval_ms: 2_000,
            write_interval_ms: 2_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ActuatorPolicy {
    pub output_min_percent: f64,
    pub output_max_percent: f64,
    pub pwm_min: u16,
    pub pwm_max: u16,
    pub startup_kick_percent: f64,
    pub startup_kick_ms: u64,
}

impl Default for ActuatorPolicy {
    fn default() -> Self {
        Self {
            output_min_percent: 0.0,
            output_max_percent: 100.0,
            pwm_min: 0,
            pwm_max: 255,
            startup_kick_percent: 35.0,
            startup_kick_ms: 1_500,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PidLimits {
    pub integral_min: f64,
    pub integral_max: f64,
    pub derivative_min: f64,
    pub derivative_max: f64,
}

impl Default for PidLimits {
    fn default() -> Self {
        Self {
            integral_min: -100.0,
            integral_max: 100.0,
            derivative_min: -50.0,
            derivative_max: 50.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoTuneProposal {
    pub proposed_gains: PidGains,
    pub observation_window_ms: u64,
    pub lag_time_ms: u64,
    pub max_rate_c_per_sec: f64,
}

impl AutoTuneProposal {
    pub fn from_step_response(
        observation_window_ms: u64,
        lag_time_ms: u64,
        max_rate_c_per_sec: f64,
    ) -> Option<Self> {
        if observation_window_ms == 0 || lag_time_ms == 0 || max_rate_c_per_sec <= 0.0 {
            return None;
        }

        let lag_seconds = lag_time_ms as f64 / 1_000.0;
        let raw_kp = 1.2 / (max_rate_c_per_sec * lag_seconds);
        let raw_ki = raw_kp / (2.0 * lag_seconds);
        let raw_kd = raw_kp * lag_seconds / 0.5;

        Some(Self {
            proposed_gains: PidGains {
                kp: raw_kp * 0.6,
                ki: raw_ki * 0.5,
                kd: raw_kd * 0.75,
            },
            observation_window_ms,
            lag_time_ms,
            max_rate_c_per_sec,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PidOutput {
    pub logical_output_percent: f64,
    pub error_millidegrees: f64,
    pub derivative_millidegrees_per_second: f64,
    pub integral_state: f64,
}

#[derive(Debug, Clone)]
pub struct PidController {
    target_temp_millidegrees: i64,
    gains: PidGains,
    limits: PidLimits,
    deadband_millidegrees: i64,
    integral_state: f64,
    last_measurement_millidegrees: Option<f64>,
    last_output_percent: Option<f64>,
    last_error_millidegrees: Option<f64>,
}

impl PidController {
    pub fn new(
        target_temp_millidegrees: i64,
        gains: PidGains,
        limits: PidLimits,
        deadband_millidegrees: i64,
    ) -> Self {
        Self {
            target_temp_millidegrees,
            gains,
            limits,
            deadband_millidegrees,
            integral_state: 0.0,
            last_measurement_millidegrees: None,
            last_output_percent: None,
            last_error_millidegrees: None,
        }
    }

    pub fn update(&mut self, aggregated_temp_millidegrees: i64, dt_seconds: f64) -> PidOutput {
        let measurement = aggregated_temp_millidegrees as f64;
        let error = measurement - self.target_temp_millidegrees as f64;
        let safe_dt = dt_seconds.max(f64::EPSILON);

        let derivative = self
            .last_measurement_millidegrees
            .map(|last| (measurement - last) / safe_dt)
            .unwrap_or(0.0)
            .clamp(self.limits.derivative_min, self.limits.derivative_max);

        if error.abs() <= self.deadband_millidegrees as f64 {
            self.last_measurement_millidegrees = Some(measurement);
            self.last_error_millidegrees = Some(error);

            return PidOutput {
                logical_output_percent: self.last_output_percent.unwrap_or(0.0),
                error_millidegrees: error,
                derivative_millidegrees_per_second: derivative,
                integral_state: self.integral_state,
            };
        }

        self.integral_state = (self.integral_state + error * safe_dt)
            .clamp(self.limits.integral_min, self.limits.integral_max);

        let output = (self.gains.kp * error
            + self.gains.ki * self.integral_state
            + self.gains.kd * derivative)
            .clamp(0.0, 100.0);

        self.last_measurement_millidegrees = Some(measurement);
        self.last_output_percent = Some(output);
        self.last_error_millidegrees = Some(error);

        PidOutput {
            logical_output_percent: output,
            error_millidegrees: error,
            derivative_millidegrees_per_second: derivative,
            integral_state: self.integral_state,
        }
    }

    pub fn last_output_percent(&self) -> Option<f64> {
        self.last_output_percent
    }

    pub fn last_error_millidegrees(&self) -> Option<f64> {
        self.last_error_millidegrees
    }
}

pub fn map_output_percent_to_pwm(percent: f64, policy: &ActuatorPolicy) -> u16 {
    let bounded_percent = percent
        .clamp(0.0, 100.0)
        .clamp(policy.output_min_percent, policy.output_max_percent);

    if policy.pwm_min == policy.pwm_max
        || (policy.output_max_percent - policy.output_min_percent).abs() < f64::EPSILON
    {
        return policy.pwm_min;
    }

    let normalized = (bounded_percent - policy.output_min_percent)
        / (policy.output_max_percent - policy.output_min_percent);
    let pwm = f64::from(policy.pwm_min)
        + normalized * f64::from(policy.pwm_max.saturating_sub(policy.pwm_min));
    pwm.round() as u16
}

pub fn startup_kick_required(last_percent: Option<f64>, next_percent: f64) -> bool {
    next_percent > 0.0 && last_percent.unwrap_or(0.0) <= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-6, "left={left}, right={right}");
    }

    #[test]
    fn control_derivative_uses_measurement_delta() {
        let mut controller = PidController::new(
            50_000,
            PidGains {
                kp: 0.0,
                ki: 0.0,
                kd: 1.0,
            },
            PidLimits {
                integral_min: -1.0,
                integral_max: 1.0,
                derivative_min: -100_000.0,
                derivative_max: 100_000.0,
            },
            0,
        );

        let first = controller.update(52_000, 1.0);
        let second = controller.update(55_000, 1.0);

        approx_eq(first.logical_output_percent, 0.0);
        approx_eq(second.derivative_millidegrees_per_second, 3_000.0);
        approx_eq(second.logical_output_percent, 100.0);
    }

    #[test]
    fn control_deadband_keeps_previous_output_stable() {
        let mut controller = PidController::new(
            50_000,
            PidGains {
                kp: 0.01,
                ki: 0.0,
                kd: 0.0,
            },
            PidLimits::default(),
            1_000,
        );

        let outside = controller.update(56_000, 1.0);
        let inside = controller.update(50_500, 1.0);

        assert!(outside.logical_output_percent > 0.0);
        approx_eq(
            inside.logical_output_percent,
            outside.logical_output_percent,
        );
    }

    #[test]
    fn control_clamps_integral_derivative_and_output() {
        let mut controller = PidController::new(
            40_000,
            PidGains {
                kp: 0.2,
                ki: 1.0,
                kd: 1.0,
            },
            PidLimits {
                integral_min: -10.0,
                integral_max: 10.0,
                derivative_min: -5.0,
                derivative_max: 5.0,
            },
            0,
        );

        controller.update(80_000, 1.0);
        let output = controller.update(120_000, 1.0);

        approx_eq(output.integral_state, 10.0);
        approx_eq(output.derivative_millidegrees_per_second, 5.0);
        approx_eq(output.logical_output_percent, 100.0);
    }

    #[test]
    fn control_maps_logical_output_to_pwm_range() {
        let policy = ActuatorPolicy {
            output_min_percent: 20.0,
            output_max_percent: 80.0,
            pwm_min: 100,
            pwm_max: 200,
            startup_kick_percent: 35.0,
            startup_kick_ms: 1_500,
        };

        assert_eq!(map_output_percent_to_pwm(0.0, &policy), 100);
        assert_eq!(map_output_percent_to_pwm(50.0, &policy), 150);
        assert_eq!(map_output_percent_to_pwm(100.0, &policy), 200);
    }

    #[test]
    fn control_startup_kick_only_on_stop_to_spin_transition() {
        assert!(startup_kick_required(None, 10.0));
        assert!(startup_kick_required(Some(0.0), 10.0));
        assert!(!startup_kick_required(Some(10.0), 20.0));
        assert!(!startup_kick_required(Some(10.0), 0.0));
    }

    #[test]
    fn control_auto_tune_softens_step_response_gains() {
        let proposal = AutoTuneProposal::from_step_response(30_000, 5_000, 2.0)
            .expect("proposal should be generated");

        approx_eq(proposal.proposed_gains.kp, 0.072);
        approx_eq(proposal.proposed_gains.ki, 0.006);
        approx_eq(proposal.proposed_gains.kd, 0.9);
    }
}
