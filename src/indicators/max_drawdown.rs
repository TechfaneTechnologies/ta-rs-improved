use std::collections::VecDeque;
use std::fmt;

use crate::errors::{Result, TaError};
use crate::indicators::AdaptiveTimeDetector;
use crate::{Next, Reset};
use chrono::{DateTime, Duration, Utc};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct MaxDrawdown {
    duration: Duration,
    window: VecDeque<(DateTime<Utc>, f64)>,
    detector: AdaptiveTimeDetector,
}

impl MaxDrawdown {
    pub fn get_window(&self) -> VecDeque<(DateTime<Utc>, f64)> {
        self.window.clone()
    }
    pub fn new(duration: Duration) -> Result<Self> {
        if duration.num_seconds() <= 0 {
            Err(TaError::InvalidParameter)
        } else {
            Ok(Self {
                duration,
                window: VecDeque::new(),
                detector: AdaptiveTimeDetector::new(),
            })
        }
    }

    fn calculate_max_drawdown(&self) -> f64 {
        let mut peak = f64::MIN;
        let mut max_drawdown = 0.0;

        for &(_, value) in &self.window {
            if value > peak {
                peak = value;
            }
            let drawdown = (peak - value) / peak;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
        }

        100.0 * max_drawdown
    }

    fn remove_old_data(&mut self, current_time: DateTime<Utc>) {
        while self
            .window
            .front()
            .map_or(false, |(time, _)| *time < current_time - self.duration)
        {
            self.window.pop_front();
        }
    }
}

impl Next<f64> for MaxDrawdown {
    type Output = f64;

    fn next(&mut self, (timestamp, value): (DateTime<Utc>, f64)) -> Self::Output {
        // Check if we should replace the last value (same time bucket)
        let should_replace = self.detector.should_replace(timestamp);

        if should_replace && !self.window.is_empty() {
            // Replace the last value in the same time bucket
            self.window.pop_back();
        } else {
            // New time period - remove old data first
            self.remove_old_data(timestamp);
        }

        // Add the new data point
        self.window.push_back((timestamp, value));

        // Calculate the maximum drawdown within the current window
        self.calculate_max_drawdown()
    }
}

impl Reset for MaxDrawdown {
    fn reset(&mut self) {
        self.window.clear();
        self.detector.reset();
    }
}

impl fmt::Display for MaxDrawdown {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MaxDrawdown({}s)", self.duration.num_seconds())
    }
}

impl Default for MaxDrawdown {
    fn default() -> Self {
        Self::new(Duration::days(14)).unwrap()
    }
}

mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn test_new() {
        assert!(MaxDrawdown::new(Duration::seconds(0)).is_err());
        assert!(MaxDrawdown::new(Duration::seconds(1)).is_ok());
    }

    #[test]
    fn test_next() {
        let duration = Duration::seconds(2);
        let mut max = MaxDrawdown::new(duration).unwrap();
        let start_time = Utc.ymd(2020, 1, 1).and_hms(0, 0, 0);

        assert_eq!(max.next((start_time, 4.0)), 0.0);
        assert_eq!(max.next((start_time + Duration::seconds(1), 2.0)), 50.0);
        assert_eq!(max.next((start_time + Duration::seconds(2), 1.0)), 75.0);
        assert_eq!(max.next((start_time + Duration::seconds(3), 3.0)), 50.0);
        assert_eq!(max.next((start_time + Duration::seconds(4), 4.0)), 0.0);
        assert_eq!(max.next((start_time + Duration::seconds(5), 0.0)), 100.0);
        assert_eq!(max.next((start_time + Duration::seconds(6), 2.0)), 100.0);
        assert_eq!(max.next((start_time + Duration::seconds(7), 3.0)), 0.0);
        assert_eq!(max.next((start_time + Duration::seconds(8), 1.5)), 50.0);
    }

    #[test]
    fn test_reset() {
        let duration = Duration::seconds(100);
        let mut max = MaxDrawdown::new(duration).unwrap();
        let start_time = Utc.ymd(2020, 1, 1).and_hms(0, 0, 0);

        assert_eq!(max.next((start_time, 4.0)), 0.0);
        assert_eq!(max.next((start_time + Duration::seconds(50), 10.0)), 0.0);
        assert_eq!(max.next((start_time + Duration::seconds(100), 2.0)), 80.0);

        max.reset();
        assert_eq!(max.next((start_time + Duration::seconds(150), 4.0)), 0.0);
    }

    #[test]
    fn test_display() {
        let indicator = MaxDrawdown::new(Duration::seconds(7)).unwrap();
        assert_eq!(format!("{}", indicator), "MaxDrawdown(7s)");
    }
}
