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
pub struct Maximum {
    duration: Duration,
    window: VecDeque<(DateTime<Utc>, f64)>,
    detector: AdaptiveTimeDetector,
}

impl Maximum {
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

    fn find_max_value(&self) -> f64 {
        self.window
            .iter()
            .map(|&(_, val)| val)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    fn remove_old_data(&mut self, current_time: DateTime<Utc>) {
        while self
            .window
            .front()
            .map_or(false, |(time, _)| *time <= current_time - self.duration)
        {
            self.window.pop_front();
        }
    }
}

impl Default for Maximum {
    fn default() -> Self {
        Self::new(Duration::days(14)).unwrap()
    }
}

impl Next<f64> for Maximum {
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

        // Find the maximum value in the current window
        self.find_max_value()
    }
}

impl Reset for Maximum {
    fn reset(&mut self) {
        self.window.clear();
        self.detector.reset();
    }
}

impl fmt::Display for Maximum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MAX({}s)", self.duration.num_seconds())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn test_new() {
        assert!(Maximum::new(Duration::seconds(0)).is_err());
        assert!(Maximum::new(Duration::seconds(1)).is_ok());
    }

    #[test]
    fn test_next() {
        let duration = Duration::seconds(2);
        let mut max = Maximum::new(duration).unwrap();
        let start_time = Utc.ymd(2020, 1, 1).and_hms(0, 0, 0);

        assert_eq!(max.next((start_time, 4.0)), 4.0);
        assert_eq!(max.next((start_time + Duration::seconds(1), 1.2)), 4.0);
        assert_eq!(max.next((start_time + Duration::seconds(2), 5.0)), 5.0);
        assert_eq!(max.next((start_time + Duration::seconds(3), 3.0)), 5.0);
        assert_eq!(max.next((start_time + Duration::seconds(4), 4.0)), 4.0);
        assert_eq!(max.next((start_time + Duration::seconds(5), 0.0)), 4.0);
        assert_eq!(max.next((start_time + Duration::seconds(6), -1.0)), 0.0);
        assert_eq!(max.next((start_time + Duration::seconds(7), -2.0)), -1.0);
        assert_eq!(max.next((start_time + Duration::seconds(8), -1.5)), -1.5);
    }

    #[test]
    fn test_reset() {
        let duration = Duration::seconds(100);
        let mut max = Maximum::new(duration).unwrap();
        let start_time = Utc.ymd(2020, 1, 1).and_hms(0, 0, 0);

        assert_eq!(max.next((start_time, 4.0)), 4.0);
        assert_eq!(max.next((start_time + Duration::seconds(50), 10.0)), 10.0);
        assert_eq!(max.next((start_time + Duration::seconds(100), 4.0)), 10.0);

        max.reset();
        assert_eq!(max.next((start_time + Duration::seconds(150), 4.0)), 4.0);
    }

    #[test]
    fn test_default() {
        let _ = Maximum::default();
    }

    #[test]
    fn test_display() {
        let indicator = Maximum::new(Duration::seconds(7)).unwrap();
        assert_eq!(format!("{}", indicator), "MAX(7s)");
    }
}
