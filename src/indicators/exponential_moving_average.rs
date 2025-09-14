use std::fmt;
use std::time::Duration;

use crate::errors::{Result, TaError};
use crate::indicators::AdaptiveTimeDetector;
use crate::{Next, Reset};
use chrono::{DateTime, Utc};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[doc(alias = "EMA")]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct ExponentialMovingAverage {
    duration: Duration, // Now std::time::Duration
    k: f64,
    window: VecDeque<(DateTime<Utc>, f64)>,
    current: f64,
    is_new: bool,
    detector: AdaptiveTimeDetector,
    last_value: f64,
}

impl ExponentialMovingAverage {
    pub fn new(duration: Duration) -> Result<Self> {
        // std::time::Duration can't be negative, so just check if it's zero
        if duration.as_secs() == 0 && duration.subsec_nanos() == 0 {
            Err(TaError::InvalidParameter)
        } else {
            // Convert duration to days for k calculation
            let days = duration.as_secs() as f64 / 86400.0;
            Ok(Self {
                duration,
                k: 2.0 / (days + 1.0),
                window: VecDeque::new(),
                current: 0.0,
                is_new: true,
                detector: AdaptiveTimeDetector::new(duration),
                last_value: 0.0,
            })
        }
    }

    fn remove_old_data(&mut self, current_time: DateTime<Utc>) {
        // EMA doesn't actually need to remove old data
        // It's a running average that only depends on the current state
        // Keeping the window for potential debugging, but not removing data
        // This was causing issues with RSI calculations

        // Original code commented out:
        while self
            .window
            .front()
            .map_or(false, |(time, _)| *time <= current_time - self.duration)
        {
            self.window.pop_front();
        }
    }
}

impl Next<f64> for ExponentialMovingAverage {
    type Output = f64;

    fn next(&mut self, (timestamp, value): (DateTime<Utc>, f64)) -> Self::Output {
        // Check if we should replace the last value (same time bucket)
        let should_replace = self.detector.should_replace(timestamp);

        if should_replace && !self.is_new {
            // Reverse the previous EMA calculation and apply new value
            // Previous: current = k * last_value + (1-k) * old_current
            // Solve for old_current: old_current = (current - k * last_value) / (1-k)
            let old_current = if (1.0 - self.k) != 0.0 {
                (self.current - self.k * self.last_value) / (1.0 - self.k)
            } else {
                self.current
            };
            self.current = (self.k * value) + ((1.0 - self.k) * old_current);
        } else {
            // New time period
            // EMA doesn't need to maintain a window or remove old data
            // It's a running average that only depends on current state

            if self.is_new {
                self.is_new = false;
                self.current = value;
            } else {
                self.current = (self.k * value) + ((1.0 - self.k) * self.current);
            }
        }

        self.last_value = value;
        self.current
    }
}

impl Reset for ExponentialMovingAverage {
    fn reset(&mut self) {
        self.window.clear();
        self.current = 0.0;
        self.is_new = true;
        self.detector.reset();
        self.last_value = 0.0;
    }
}

impl Default for ExponentialMovingAverage {
    fn default() -> Self {
        // Use std::time::Duration constructor
        Self::new(Duration::from_secs(14 * 24 * 60 * 60)).unwrap() // 14 days in seconds
    }
}

impl fmt::Display for ExponentialMovingAverage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let days = self.duration.as_secs() / 86400;
        if days > 0 && self.duration.as_secs() % 86400 == 0 {
            write!(f, "EMA({} days)", days)
        } else {
            write!(f, "EMA({}s)", self.duration.as_secs())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_new() {
        assert!(ExponentialMovingAverage::new(Duration::from_secs(0)).is_err());
        assert!(ExponentialMovingAverage::new(Duration::from_secs(86400)).is_ok());
        // 1 day
    }

    #[test]
    fn test_next() {
        let mut ema = ExponentialMovingAverage::new(Duration::from_secs(3 * 86400)).unwrap(); // 3 days
        let now = Utc::now();

        assert_eq!(ema.next((now, 2.0)), 2.0);
        assert_eq!(ema.next((now + chrono::Duration::days(1), 5.0)), 3.5);
        assert_eq!(ema.next((now + chrono::Duration::days(2), 1.0)), 2.25);
        assert_eq!(ema.next((now + chrono::Duration::days(3), 6.25)), 4.25);
    }

    #[test]
    fn test_reset() {
        let mut ema = ExponentialMovingAverage::new(Duration::from_secs(5 * 86400)).unwrap(); // 5 days
        let now = Utc::now();

        assert_eq!(ema.next((now, 4.0)), 4.0);
        ema.next((now + chrono::Duration::days(1), 10.0));
        ema.next((now + chrono::Duration::days(2), 15.0));
        ema.next((now + chrono::Duration::days(3), 20.0));
        assert_ne!(ema.next((now + chrono::Duration::days(4), 4.0)), 4.0);

        ema.reset();
        assert_eq!(ema.next((now, 4.0)), 4.0);
    }

    #[test]
    fn test_default() {
        let _ema = ExponentialMovingAverage::default();
    }

    #[test]
    fn test_display() {
        let ema = ExponentialMovingAverage::new(Duration::from_secs(7 * 86400)).unwrap(); // 7 days
        assert_eq!(format!("{}", ema), "EMA(7 days)");
    }
}
