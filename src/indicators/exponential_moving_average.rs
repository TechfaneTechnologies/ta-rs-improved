use std::fmt;
use std::time::Duration; // Change: Use std::time::Duration

use crate::errors::{Result, TaError};
use crate::{Next, Reset};
use chrono::{DateTime, Utc};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[doc(alias = "EMA")]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct ExponentialMovingAverage {
    period: usize,
    period_duration: Duration, // Now std::time::Duration
    k: f64,
    current: f64,
    is_new: bool,
    last_value: f64,
    last_timestamp: Option<DateTime<Utc>>,
    last_adjusted_k: f64,
}

impl ExponentialMovingAverage {
    /// Creates a new Exponential Moving Average with the specified period and duration.
    ///
    /// # Arguments
    /// * `period` - The number of periods for the EMA calculation (e.g., 14)
    /// * `period_duration` - The time duration each period represents (e.g., 1 day for daily EMA)
    ///
    /// # Example
    /// ```
    /// // 14-day EMA that can handle any frequency data
    /// let ema = ExponentialMovingAverage::new(14, Duration::from_secs(86400))?;
    /// ```
    pub fn new(period: usize, period_duration: Duration) -> Result<Self> {
        if period == 0 || (period_duration.as_secs() == 0 && period_duration.subsec_nanos() == 0) {
            Err(TaError::InvalidParameter)
        } else {
            Ok(Self {
                period,
                period_duration,
                k: 2.0 / (period as f64 + 1.0),
                current: 0.0,
                is_new: true,
                last_value: 0.0,
                last_timestamp: None,
                last_adjusted_k: 0.0,
            })
        }
    }

    /// Creates a period-based EMA (assumes each data point is one period)
    /// This is for backward compatibility
    pub fn new_period_based(period: usize) -> Result<Self> {
        // Default to daily periods for backward compatibility
        Self::new(period, Duration::from_secs(86400)) // 1 day in seconds
    }

    /// Calculate the adjusted smoothing constant based on actual time elapsed
    fn calculate_adjusted_k(&self, timestamp: DateTime<Utc>) -> f64 {
        if let Some(last_ts) = self.last_timestamp {
            let time_elapsed = timestamp - last_ts;

            // Prevent negative or zero time
            if time_elapsed.num_seconds() <= 0 {
                return 0.0; // No update if time hasn't moved forward
            }

            // Calculate how many periods have elapsed
            let periods_elapsed =
                time_elapsed.num_seconds() as f64 / self.period_duration.as_secs() as f64;

            // Adjust k using the formula: adjusted_k = 1 - (1 - k)^periods_elapsed
            // This ensures proper exponential decay regardless of time interval
            1.0 - (1.0 - self.k).powf(periods_elapsed)
        } else {
            // First data point - use full weight
            1.0
        }
    }

    /// Returns the smoothing constant (alpha) used in the EMA calculation
    pub fn smoothing_constant(&self) -> f64 {
        self.k
    }

    /// Returns the period of the EMA
    pub fn period(&self) -> usize {
        self.period
    }

    /// Returns the period duration of the EMA
    pub fn period_duration(&self) -> Duration {
        self.period_duration
    }
}

impl Next<f64> for ExponentialMovingAverage {
    type Output = f64;

    fn next(&mut self, (timestamp, value): (DateTime<Utc>, f64)) -> Self::Output {
        // Simple check: is this the exact same timestamp as last time?
        let is_replacement = self.last_timestamp == Some(timestamp);

        if is_replacement && !self.is_new {
            // Same timestamp - replace the last value
            // Reverse the last calculation and apply the new value
            if self.last_adjusted_k > 0.0 && self.last_adjusted_k < 1.0 {
                // Reverse: old_current = (current - last_adjusted_k * last_value) / (1 - last_adjusted_k)
                let old_current = (self.current - self.last_adjusted_k * self.last_value)
                    / (1.0 - self.last_adjusted_k);
                // Recalculate with new value
                self.current =
                    self.last_adjusted_k * value + (1.0 - self.last_adjusted_k) * old_current;
            } else if self.last_adjusted_k >= 1.0 {
                // Full replacement
                self.current = value;
            }
            // else last_adjusted_k is 0, keep current value unchanged

            self.last_value = value;
            // Don't update timestamp or adjusted_k for replacements
        } else {
            // New timestamp - calculate time-weighted update
            let adjusted_k = self.calculate_adjusted_k(timestamp);

            if self.is_new {
                self.is_new = false;
                self.current = value;
                self.last_adjusted_k = 1.0; // First value uses full weight
            } else if adjusted_k > 0.0 {
                // Apply time-weighted EMA formula
                self.current = adjusted_k * value + (1.0 - adjusted_k) * self.current;
                self.last_adjusted_k = adjusted_k; // Store for potential replacements
            }
            // If adjusted_k is 0 (no time elapsed), keep current value unchanged

            // Update timestamp and last_value for next calculation
            self.last_timestamp = Some(timestamp);
            self.last_value = value;
        }

        self.current
    }
}

impl Reset for ExponentialMovingAverage {
    fn reset(&mut self) {
        self.current = 0.0;
        self.is_new = true;
        self.last_value = 0.0;
        self.last_timestamp = None;
        self.last_adjusted_k = 0.0;
    }
}

impl Default for ExponentialMovingAverage {
    fn default() -> Self {
        // 14-day EMA by default
        Self::new(14, Duration::from_secs(14 * 86400)).unwrap()
    }
}

impl fmt::Display for ExponentialMovingAverage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let secs = self.period_duration.as_secs();
        if secs >= 86400 && secs % 86400 == 0 {
            write!(f, "EMA({} x {} days)", self.period, secs / 86400)
        } else if secs >= 3600 && secs % 3600 == 0 {
            write!(f, "EMA({} x {} hours)", self.period, secs / 3600)
        } else if secs >= 60 && secs % 60 == 0 {
            write!(f, "EMA({} x {} min)", self.period, secs / 60)
        } else {
            write!(f, "EMA({} x {} sec)", self.period, secs)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_new() {
        assert!(ExponentialMovingAverage::new(0, Duration::from_secs(86400)).is_err());
        assert!(ExponentialMovingAverage::new(1, Duration::from_secs(0)).is_err());
        assert!(ExponentialMovingAverage::new(1, Duration::from_secs(86400)).is_ok());
        assert!(ExponentialMovingAverage::new(14, Duration::from_secs(3600)).is_ok());
    }

    #[test]
    fn test_time_weighted_daily() {
        // 3-period daily EMA
        let mut ema = ExponentialMovingAverage::new(3, Duration::from_secs(86400)).unwrap();
        let now = Utc::now();

        // First value initializes
        assert_eq!(ema.next((now, 2.0)), 2.0);

        // One day later: full period weight
        // k = 0.5, so: 0.5 * 5.0 + 0.5 * 2.0 = 3.5
        assert_eq!(ema.next((now + chrono::Duration::days(1), 5.0)), 3.5);

        // One more day: another full period
        // 0.5 * 1.0 + 0.5 * 3.5 = 2.25
        assert_eq!(ema.next((now + chrono::Duration::days(2), 1.0)), 2.25);
    }

    #[test]
    fn test_time_weighted_mixed_frequency() {
        // 3-period daily EMA, but we'll feed it mixed frequency data
        let mut ema = ExponentialMovingAverage::new(3, Duration::from_secs(86400)).unwrap();
        let now = Utc::now();

        // Initialize with daily data
        assert_eq!(ema.next((now, 100.0)), 100.0);

        // One day later: k=0.5, so 0.5*110 + 0.5*100 = 105
        let day1_value = ema.next((now + chrono::Duration::days(1), 110.0));
        assert_eq!(day1_value, 105.0);

        // Now switch to hourly data (1/24 of a period)
        // The weight should be much smaller
        let hourly_value = ema.next((
            now + chrono::Duration::days(1) + chrono::Duration::hours(1),
            120.0,
        ));

        // After just 1 hour, the EMA shouldn't change much
        // hourly k = 1 - (1-0.5)^(1/24) ≈ 0.0283
        // 0.0283*120 + 0.9717*105 ≈ 105.4
        assert!(hourly_value > 105.0 && hourly_value < 107.0);

        // Feed 23 more hours of 120.0
        let mut current = hourly_value;
        for i in 2..=24 {
            current = ema.next((
                now + chrono::Duration::days(1) + chrono::Duration::hours(i),
                120.0,
            ));
        }

        // After 24 hours of 120.0, the compounded effect should be close to one daily update
        // One daily update from 105 to 120 with k=0.5 would give: 0.5*120 + 0.5*105 = 112.5
        assert!((current - 112.5).abs() < 1.0); // Should be approximately 112.5
    }

    #[test]
    fn test_minute_data_on_daily_ema() {
        // 14-day EMA receiving minute data
        let mut ema = ExponentialMovingAverage::new(14, Duration::from_secs(14 * 86400)).unwrap();
        let now = Utc::now();

        // Initialize
        ema.next((now, 100.0));

        // Feed minute data for an hour with increasing values
        for i in 1..=60 {
            ema.next((now + chrono::Duration::minutes(i), 100.0 + i as f64));
        }

        let after_hour = ema.current;

        // After just 1 hour (1/24 of a day), the 14-day EMA should barely move
        // Even with values going from 100 to 160
        assert!(after_hour < 105.0); // Should be very close to starting value
    }

    #[test]
    fn test_no_time_change() {
        let mut ema = ExponentialMovingAverage::new(5, Duration::from_secs(5 * 60)).unwrap();
        let base_time = Utc.ymd(2024, 1, 1).and_hms(0, 0, 0);

        // Initialize
        let v0 = ema.next((base_time, 100.0));
        assert_eq!(v0, 100.0);

        // Move forward exactly 5 minutes (one period)
        let time1 = base_time + chrono::Duration::minutes(5);
        let v1 = ema.next((time1, 110.0));

        // Now feed the EXACT same timestamp again with a different value
        let v2 = ema.next((time1, 115.0));

        println!("v1 (110): {}, v2 (115): {}", v1, v2);

        // The values MUST be different if replacement is working
        assert_ne!(
            v1, v2,
            "Same timestamp with different values must produce different results"
        );
        assert!(v2 > v1, "Higher replacement value should yield higher EMA");
    }

    #[test]
    fn test_reset() {
        let mut ema = ExponentialMovingAverage::new(5, Duration::from_secs(86400)).unwrap();
        let now = Utc::now();

        assert_eq!(ema.next((now, 4.0)), 4.0);
        ema.next((now + chrono::Duration::days(1), 10.0));

        ema.reset();
        assert_eq!(ema.next((now, 4.0)), 4.0);
        assert!(ema.last_timestamp.is_some());
    }

    #[test]
    fn test_display() {
        let ema1 = ExponentialMovingAverage::new(14, Duration::from_secs(86400)).unwrap();
        assert_eq!(format!("{}", ema1), "EMA(14 x 1 days)");

        let ema2 = ExponentialMovingAverage::new(20, Duration::from_secs(4 * 3600)).unwrap();
        assert_eq!(format!("{}", ema2), "EMA(20 x 4 hours)");

        let ema3 = ExponentialMovingAverage::new(50, Duration::from_secs(5 * 60)).unwrap();
        assert_eq!(format!("{}", ema3), "EMA(50 x 5 min)");
    }
}
