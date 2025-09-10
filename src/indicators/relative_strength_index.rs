use std::collections::VecDeque;
use std::fmt;

use crate::errors::Result;
use crate::indicators::{AdaptiveTimeDetector, ExponentialMovingAverage as Ema};
use crate::{Next, Reset};
use chrono::{DateTime, Duration, Utc};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[doc(alias = "RSI")]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct RelativeStrengthIndex {
    duration: Duration,
    up_ema_indicator: Ema,
    down_ema_indicator: Ema,
    window: VecDeque<(DateTime<Utc>, f64)>, // Store tuples of (timestamp, value)
    prev_val: Option<f64>,
    detector: AdaptiveTimeDetector,
}

impl RelativeStrengthIndex {
    pub fn new(duration: Duration) -> Result<Self> {
        Ok(Self {
            duration,
            up_ema_indicator: Ema::new(duration)?,
            down_ema_indicator: Ema::new(duration)?,
            window: VecDeque::new(),
            prev_val: None,
            detector: AdaptiveTimeDetector::new(),
        })
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

impl Next<f64> for RelativeStrengthIndex {
    type Output = f64;

    fn next(&mut self, (timestamp, value): (DateTime<Utc>, f64)) -> Self::Output {
        // Check if we should replace the last value (same time bucket)
        let should_replace = self.detector.should_replace(timestamp);
        
        if should_replace && !self.window.is_empty() {
            // For RSI, when replacing a value in the same time bucket,
            // we don't change prev_val since it represents the previous period's close
            // Just remove the last window entry to be replaced
            self.window.pop_back();
        } else {
            // New time period - remove old data first
            self.remove_old_data(timestamp);
            
            // Update prev_val to the last complete period's value
            // This is crucial: prev_val should be the closing value of the previous period
            if !self.window.is_empty() {
                self.prev_val = Some(self.window.back().unwrap().1);
            }
        }

        // Calculate gain and loss using the stable prev_val
        let (gain, loss) = if let Some(prev_val) = self.prev_val {
            if value > prev_val {
                (value - prev_val, 0.0)
            } else {
                (0.0, prev_val - value)
            }
        } else {
            (0.0, 0.0)
        };

        // Add to window AFTER calculating gain/loss
        self.window.push_back((timestamp, value));
        
        // Only update prev_val for the NEXT period if this is not a replacement
        // When replacing, prev_val stays as the previous period's close
        if !should_replace {
            self.prev_val = Some(value);
        }

        // Update EMAs
        let avg_up = self.up_ema_indicator.next((timestamp, gain));
        let avg_down = self.down_ema_indicator.next((timestamp, loss));

        // Calculate and return RSI
        if avg_down == 0.0 {
            if avg_up == 0.0 {
                50.0 // Neutral value when no movement
            } else {
                100.0 // Max value when only gains
            }
        } else {
            let rs = avg_up / avg_down;
            100.0 - (100.0 / (1.0 + rs))
        }
    }
}

impl Reset for RelativeStrengthIndex {
    fn reset(&mut self) {
        self.window.clear();
        self.prev_val = None;
        self.up_ema_indicator.reset();
        self.down_ema_indicator.reset();
        self.detector.reset();
    }
}

impl Default for RelativeStrengthIndex {
    fn default() -> Self {
        Self::new(Duration::days(14)).unwrap()
    }
}

impl fmt::Display for RelativeStrengthIndex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RSI({:?} days)", self.duration.num_days())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helper::*;
    use chrono::{TimeZone, Utc};

    test_indicator!(RelativeStrengthIndex);

    #[test]
    fn test_new() {
        assert!(RelativeStrengthIndex::new(Duration::days(0)).is_err());
        assert!(RelativeStrengthIndex::new(Duration::days(1)).is_ok());
    }

    #[test]
    fn test_next() {
        let mut rsi = RelativeStrengthIndex::new(Duration::days(3)).unwrap();
        let timestamp = Utc.ymd(2020, 1, 1).and_hms(0, 0, 0);
        
        // First value: 10.0 (no previous value, so RSI = 50)
        assert_eq!(rsi.next((timestamp, 10.0)), 50.0);
        
        // Second value: 10.5 (gain of 0.5, no loss)
        assert_eq!(
            rsi.next((timestamp + Duration::days(1), 10.5)).round(),
            100.0
        );
        
        // Third value: 10.0 (loss of 0.5 from 10.5)
        // With EMA k=0.5: avg_up=0.125, avg_down=0.25, RS=0.5, RSI=33.33
        assert_eq!(
            rsi.next((timestamp + Duration::days(2), 10.0)).round(),
            33.0
        );
        
        // Fourth value: 9.5 (loss of 0.5 from 10.0)
        // With continued losses, RSI should drop further
        // avg_up = 0.0625, avg_down = 0.375, RS = 0.1667, RSI = 14.3
        assert_eq!(rsi.next((timestamp + Duration::days(3), 9.5)).round(), 14.0);
    }

    #[test]
    fn test_reset() {
        let mut rsi = RelativeStrengthIndex::new(Duration::days(3)).unwrap();
        let timestamp = Utc.ymd(2020, 1, 1).and_hms(0, 0, 0);
        assert_eq!(rsi.next((timestamp, 10.0)), 50.0);
        assert_eq!(
            rsi.next((timestamp + Duration::days(1), 10.5)).round(),
            100.0
        );

        rsi.reset();
        assert_eq!(rsi.next((timestamp, 10.0)).round(), 50.0);
        assert_eq!(
            rsi.next((timestamp + Duration::days(1), 10.5)).round(),
            100.0
        );
    }

    #[test]
    fn test_default() {
        RelativeStrengthIndex::default();
    }

    #[test]
    fn test_display() {
        let rsi = RelativeStrengthIndex::new(Duration::days(16)).unwrap();
        assert_eq!(format!("{}", rsi), "RSI(16 days)");
    }
}
