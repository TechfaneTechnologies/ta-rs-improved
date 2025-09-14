use chrono::{DateTime, Datelike, Utc};
use std::collections::VecDeque;
use std::time::Duration; // Change: Use std::time::Duration

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Represents the detected frequency of incoming data
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum DetectedFrequency {
    /// Still learning from initial data points
    Unknown,
    /// Determined to be daily OHLC data (no de-duplication needed)
    DailyOHLC,
    /// Determined to be intraday data with specific bucket size for de-duplication
    Intraday(Duration), // Now std::time::Duration
}

/// Handles adaptive time detection and de-duplication logic for indicators
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct AdaptiveTimeDetector {
    frequency: DetectedFrequency,
    timestamp_history: VecDeque<DateTime<Utc>>,
    last_processed_bucket: i64,
    detection_samples: usize,
}

impl AdaptiveTimeDetector {
    /// Create a new adaptive time detector
    pub fn new() -> Self {
        Self {
            frequency: DetectedFrequency::Unknown,
            timestamp_history: VecDeque::with_capacity(5),
            last_processed_bucket: i64::MIN,
            detection_samples: 3, // Need at least 3 samples to detect frequency
        }
    }

    /// Create a new detector with custom detection samples
    pub fn with_samples(detection_samples: usize) -> Self {
        Self {
            frequency: DetectedFrequency::Unknown,
            timestamp_history: VecDeque::with_capacity(detection_samples + 2),
            last_processed_bucket: i64::MIN,
            detection_samples: detection_samples.max(2),
        }
    }

    /// Get the current detected frequency
    pub fn frequency(&self) -> &DetectedFrequency {
        &self.frequency
    }

    /// Detect frequency from collected timestamp history
    fn detect_frequency(&mut self) {
        if self.timestamp_history.len() < self.detection_samples {
            return;
        }

        // Calculate average time delta between consecutive timestamps
        let mut total_delta_seconds = 0i64;
        let mut count = 0;

        for i in 1..self.timestamp_history.len() {
            let delta = self.timestamp_history[i] - self.timestamp_history[i - 1];
            total_delta_seconds += delta.num_seconds();
            count += 1;
        }

        if count == 0 {
            return;
        }

        let avg_delta_seconds = total_delta_seconds / count as i64;
        let avg_delta = Duration::from_secs(avg_delta_seconds as u64);

        // Apply heuristics to distinguish data patterns
        if avg_delta > Duration::from_secs(4 * 3600) {
            // 4 hours
            // Data points are more than 4 hours apart - likely daily OHLC
            self.frequency = DetectedFrequency::DailyOHLC;
        } else if avg_delta < Duration::from_secs(30) {
            // Data points are less than 30 seconds apart - likely test data or tick data
            self.frequency = DetectedFrequency::Intraday(Duration::from_secs(1));
        } else {
            // Intraday data - round to sensible bucket sizes
            let bucket_duration = if avg_delta < Duration::from_secs(2 * 60) {
                // 2 minutes
                Duration::from_secs(60) // 1-minute buckets
            } else if avg_delta < Duration::from_secs(10 * 60) {
                // 10 minutes
                Duration::from_secs(5 * 60) // 5-minute buckets
            } else if avg_delta < Duration::from_secs(30 * 60) {
                // 30 minutes
                Duration::from_secs(15 * 60) // 15-minute buckets
            } else {
                Duration::from_secs(3600) // Hourly buckets
            };
            self.frequency = DetectedFrequency::Intraday(bucket_duration);
        }
    }

    /// Calculate the time bucket for a given timestamp based on detected frequency
    fn calculate_bucket(&self, timestamp: DateTime<Utc>) -> i64 {
        match &self.frequency {
            DetectedFrequency::DailyOHLC => {
                // For daily OHLC, each timestamp is unique (no bucketing)
                timestamp.timestamp()
            }
            DetectedFrequency::Intraday(bucket_size) => {
                // Divide timestamp by bucket size to group into intervals
                timestamp.timestamp() / bucket_size.as_secs() as i64
            }
            DetectedFrequency::Unknown => {
                // Each timestamp is unique until we detect frequency
                timestamp.timestamp()
            }
        }
    }

    /// Process a new timestamp and determine if it should replace the previous value
    /// Returns true if this is a duplicate within the same time bucket (should replace)
    /// Returns false if this is a new time period (should append)
    pub fn should_replace(&mut self, timestamp: DateTime<Utc>) -> bool {
        // Add to history for frequency detection
        if self.frequency == DetectedFrequency::Unknown {
            self.timestamp_history.push_back(timestamp);

            // Try to detect frequency once we have enough samples
            if self.timestamp_history.len() >= self.detection_samples {
                self.detect_frequency();
            }
            // While learning, don't replace anything
            return false;
        }

        // For DailyOHLC: NEVER replace - Open and Close are both valid
        if self.frequency == DetectedFrequency::DailyOHLC {
            return false;
        }

        // For Intraday: Apply bucket-based de-duplication
        let current_bucket = self.calculate_bucket(timestamp);

        // Check if we're in the same bucket as last processed
        let should_replace = current_bucket == self.last_processed_bucket;

        // Update last processed bucket
        self.last_processed_bucket = current_bucket;

        should_replace
    }

    /// Reset the detector to initial state
    pub fn reset(&mut self) {
        self.frequency = DetectedFrequency::Unknown;
        self.timestamp_history.clear();
        self.last_processed_bucket = i64::MIN;
    }

    /// Check if frequency has been detected
    pub fn is_detected(&self) -> bool {
        self.frequency != DetectedFrequency::Unknown
    }
}

impl Default for AdaptiveTimeDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_daily_detection() {
        let mut detector = AdaptiveTimeDetector::new();
        let base = Utc.ymd(2024, 1, 1).and_hms(9, 30, 0);

        // Simulate daily OHLC data (9:30 AM and 4:00 PM)
        assert!(!detector.should_replace(base)); // First point
        assert!(!detector
            .should_replace(base + chrono::Duration::hours(6) + chrono::Duration::minutes(30)));
        assert!(!detector.should_replace(base + chrono::Duration::days(1)));

        // Frequency should now be detected as DailyOHLC
        assert_eq!(detector.frequency(), &DetectedFrequency::DailyOHLC);

        // With de-duplication disabled, no values should be replaced
        assert!(!detector.should_replace(
            base + chrono::Duration::days(1)
                + chrono::Duration::hours(6)
                + chrono::Duration::minutes(30)
        ));
    }

    #[test]
    fn test_intraday_detection() {
        let mut detector = AdaptiveTimeDetector::new();
        let base = Utc.ymd(2024, 1, 1).and_hms(9, 30, 0);

        // Simulate 5-minute data
        assert!(!detector.should_replace(base));
        assert!(!detector.should_replace(base + chrono::Duration::minutes(5)));
        assert!(!detector.should_replace(base + chrono::Duration::minutes(10)));

        // Should detect as 5-minute intraday
        assert!(matches!(
            detector.frequency(),
            DetectedFrequency::Intraday(d) if d.as_secs() == 5 * 60
        ));

        // Next 5-minute period should not replace
        assert!(!detector.should_replace(base + chrono::Duration::minutes(15)));

        // Within same 5-minute bucket
        assert!(detector.should_replace(base + chrono::Duration::minutes(16)));
    }

    #[test]
    fn test_transition_from_daily_to_intraday() {
        let mut detector = AdaptiveTimeDetector::new();
        let base = Utc.ymd(2024, 1, 1).and_hms(9, 30, 0);

        // Start with daily data
        detector.should_replace(base);
        detector.should_replace(base + chrono::Duration::hours(6) + chrono::Duration::minutes(30));
        detector.should_replace(base + chrono::Duration::days(1));

        assert_eq!(detector.frequency(), &DetectedFrequency::DailyOHLC);

        // Once detected as DailyOHLC, it never replaces values
        assert!(!detector
            .should_replace(base + chrono::Duration::days(1) + chrono::Duration::minutes(1)));
        assert!(!detector
            .should_replace(base + chrono::Duration::days(1) + chrono::Duration::minutes(2)));
    }

    #[test]
    fn test_reset() {
        let mut detector = AdaptiveTimeDetector::new();
        let base = Utc.ymd(2024, 1, 1).and_hms(9, 30, 0);

        // Detect daily frequency
        detector.should_replace(base);
        detector.should_replace(base + chrono::Duration::hours(7));
        detector.should_replace(base + chrono::Duration::days(1));
        assert!(detector.is_detected());

        // Reset
        detector.reset();
        assert!(!detector.is_detected());
        assert_eq!(detector.frequency(), &DetectedFrequency::Unknown);
    }
}
