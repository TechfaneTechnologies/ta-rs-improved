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
pub struct MaxDrawup {
    duration: Duration,
    window: VecDeque<(DateTime<Utc>, f64)>,
    detector: AdaptiveTimeDetector,
}

impl MaxDrawup {
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

    fn calculate_max_drawup(&self) -> f64 {
        let mut trough = f64::MAX;
        let mut max_drawup = 0.0;

        for &(_, value) in &self.window {
            if value < trough {
                trough = value;
            }
            let drawup = (value - trough) / trough;
            if drawup > max_drawup {
                max_drawup = drawup;
            }
        }

        100.0 * max_drawup
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

impl Next<f64> for MaxDrawup {
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

        // Calculate the maximum drawup within the current window
        self.calculate_max_drawup()
    }
}

impl Reset for MaxDrawup {
    fn reset(&mut self) {
        self.window.clear();
        self.detector.reset();
    }
}

impl fmt::Display for MaxDrawup {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MaxDrawup({}s)", self.duration.num_seconds())
    }
}

impl Default for MaxDrawup {
    fn default() -> Self {
        Self::new(Duration::days(14)).unwrap()
    }
}

mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn test_new() {
        assert!(MaxDrawup::new(Duration::seconds(0)).is_err());
        assert!(MaxDrawup::new(Duration::seconds(1)).is_ok());
    }

    #[test]
    fn test_next() {
        let duration = Duration::seconds(2);
        let mut max = MaxDrawup::new(duration).unwrap();
        let start_time = Utc.ymd(2020, 1, 1).and_hms(0, 0, 0);

        assert_eq!(max.next((start_time, 4.0)), 0.0);
        assert_eq!(max.next((start_time + Duration::seconds(1), 2.0)), 0.0);
        assert_eq!(max.next((start_time + Duration::seconds(2), 1.0)), 0.0);
        assert_eq!(max.next((start_time + Duration::seconds(3), 3.0)), 200.0);
        assert_eq!(max.next((start_time + Duration::seconds(4), 4.0)), 300.0);
        assert_eq!(
            crate::test_helper::round(max.next((start_time + Duration::seconds(5), 3.0))),
            33.333
        );
        assert_eq!(max.next((start_time + Duration::seconds(6), 6.0)), 100.0);
        assert_eq!(max.next((start_time + Duration::seconds(7), 9.0)), 200.0);
    }

    #[test]
    fn test_reset() {
        let duration = Duration::seconds(100);
        let mut max_drawup = MaxDrawup::new(duration).unwrap();
        let start_time = Utc.ymd(2020, 1, 1).and_hms(0, 0, 0);

        assert_eq!(max_drawup.next((start_time, 4.0)), 0.0);

        assert_eq!(
            max_drawup.next((start_time + Duration::seconds(50), 10.0)),
            150.0
        );

        assert_eq!(
            max_drawup.next((start_time + Duration::seconds(100), 2.0)),
            150.0
        );

        max_drawup.reset();

        assert_eq!(
            max_drawup.next((start_time + Duration::seconds(150), 4.0)),
            0.0
        );

        assert_eq!(
            max_drawup.next((start_time + Duration::seconds(200), 8.0)),
            100.0
        );
    }

    #[test]
    fn test_display() {
        let indicator = MaxDrawup::new(Duration::seconds(7)).unwrap();
        assert_eq!(format!("{}", indicator), "MaxDrawup(7s)");
    }
}
