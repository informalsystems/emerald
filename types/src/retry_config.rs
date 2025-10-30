use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Exponential backoff retry configuration
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Initial delay between retry attempts
    /// Supports human-readable format: "100ms", "1s", "500ms", etc.
    #[serde(with = "humantime_serde")]
    pub initial_delay: Duration,

    /// Maximum delay between retry attempts
    /// Supports human-readable format: "2s", "5s", "10s", etc.
    #[serde(with = "humantime_serde")]
    pub max_delay: Duration,

    /// Total timeout - maximum time to keep retrying
    /// Supports human-readable format: "10s", "1m", "30s", etc.
    #[serde(with = "humantime_serde")]
    pub max_elapsed_time: Duration,

    /// Exponential backoff multiplier (e.g., 2.0 for doubling)
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,
}

fn default_multiplier() -> f64 {
    2.0
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(2),
            max_elapsed_time: Duration::from_secs(10),
            multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Calculate the next retry delay using exponential backoff
    pub fn next_delay(&self, current_delay: Duration) -> Duration {
        let next = current_delay.mul_f64(self.multiplier);
        std::cmp::min(next, self.max_delay)
    }
}
