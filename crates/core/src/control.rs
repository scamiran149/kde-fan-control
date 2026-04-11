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
