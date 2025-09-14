use std::collections::VecDeque;
use std::fmt;

use crate::indicators::AdaptiveTimeDetector;
use crate::Next;
use crate::{errors::Result, Reset};
use chrono::{DateTime, Duration, Utc};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[doc(alias = "SMA")]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct SimpleMovingAverage {
    duration: Duration,
    window: VecDeque<(DateTime<Utc>, f64)>,
    sum: f64,
    detector: AdaptiveTimeDetector,
}

impl SimpleMovingAverage {
    pub fn get_window(&self) -> VecDeque<(DateTime<Utc>, f64)> {
        self.window.clone()
    }
    pub fn new(duration: Duration) -> Result<Self> {
        if duration.num_seconds() <= 0 {
            return Err(crate::errors::TaError::InvalidParameter);
        }
        Ok(Self {
            duration,
            window: VecDeque::new(),
            sum: 0.0,
            detector: AdaptiveTimeDetector::new(),
        })
    }

    pub fn get_internal_state(&self) -> (Duration, VecDeque<(DateTime<Utc>, f64)>, f64) {
        (self.duration, self.window.clone(), self.sum)
    }

    fn remove_old_data(&mut self, current_time: DateTime<Utc>) {
        while self
            .window
            .front()
            .map_or(false, |(time, _)| *time <= current_time - self.duration)
        {
            if let Some((_, value)) = self.window.pop_front() {
                self.sum -= value;
            }
        }
    }
}

impl Next<f64> for SimpleMovingAverage {
    type Output = f64;

    fn next(&mut self, (timestamp, value): (DateTime<Utc>, f64)) -> Self::Output {
        // Check if we should replace the last value (same time bucket)
        let should_replace = self.detector.should_replace(timestamp);

        if should_replace && !self.window.is_empty() {
            // Replace the last value in the same time bucket
            if let Some((_, old_value)) = self.window.pop_back() {
                self.sum -= old_value;
            }
        } else {
            // New time period - remove old data first
            self.remove_old_data(timestamp);
        }

        // Add new data point
        self.window.push_back((timestamp, value));
        self.sum += value;

        // Calculate moving average
        if self.window.is_empty() {
            0.0
        } else {
            self.sum / self.window.len() as f64
        }
    }
}

impl Reset for SimpleMovingAverage {
    fn reset(&mut self) {
        self.window.clear();
        self.sum = 0.0;
        self.detector.reset();
    }
}

impl Default for SimpleMovingAverage {
    fn default() -> Self {
        Self::new(Duration::days(14)).unwrap()
    }
}

impl fmt::Display for SimpleMovingAverage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SMA({:?})", self.duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_new() {
        assert!(SimpleMovingAverage::new(Duration::seconds(0)).is_err());
        assert!(SimpleMovingAverage::new(Duration::seconds(1)).is_ok());
    }

    #[test]
    fn test_next() {
        let duration = Duration::seconds(4);
        let mut sma = SimpleMovingAverage::new(duration).unwrap();
        let start_time = Utc::now();
        let elapsed_time = Duration::seconds(1);
        assert_eq!(sma.next((start_time, 4.0)), 4.0);
        assert_eq!(sma.next((start_time + elapsed_time, 5.0)), 4.5);
        assert_eq!(sma.next((start_time + elapsed_time * 2, 6.0)), 5.0);
        assert_eq!(sma.next((start_time + elapsed_time * 3, 6.0)), 5.25);
        assert_eq!(sma.next((start_time + elapsed_time * 4, 6.0)), 5.75);
        assert_eq!(sma.next((start_time + elapsed_time * 5, 6.0)), 6.0);
        assert_eq!(sma.next((start_time + elapsed_time * 6, 2.0)), 5.0);
        // test explicit out of bounds
        assert_eq!(
            sma.next((start_time + elapsed_time * 6 + duration, 2.0)),
            2.0
        );
    }

    #[test]
    fn test_reset() {
        let duration = Duration::seconds(4);
        let mut sma = SimpleMovingAverage::new(duration).unwrap();
        let start_time = Utc::now();
        let elapsed_time = Duration::seconds(1);
        assert_eq!(sma.next((start_time, 4.0)), 4.0);
        assert_eq!(sma.next((start_time + elapsed_time, 5.0)), 4.5);
        assert_eq!(sma.next((start_time + elapsed_time * 2, 6.0)), 5.0);

        sma.reset();
        assert_eq!(sma.next((start_time + elapsed_time * 3, 99.0)), 99.0);
    }

    #[test]
    fn test_default() {
        let _sma = SimpleMovingAverage::default();
    }

    #[test]
    fn test_display() {
        let duration = Duration::seconds(5);
        let sma = SimpleMovingAverage::new(duration).unwrap();
        assert_eq!(format!("{}", sma), format!("SMA({:?})", duration));
    }
}
