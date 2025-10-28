use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Serde helpers for Duration serialization as milliseconds
mod duration_ms {
    use super::*;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

/// Exponential backoff retry configuration
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Initial delay between retry attempts (in milliseconds)
    #[serde(with = "duration_ms")]
    pub initial_delay: Duration,

    /// Maximum delay between retry attempts (in milliseconds)
    #[serde(with = "duration_ms")]
    pub max_delay: Duration,

    /// Total timeout - maximum time to keep retrying (in milliseconds)
    #[serde(with = "duration_ms")]
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
