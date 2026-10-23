//! Rolling window of app-observed input event intervals and simple statistics.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// The number of retained interval samples. This matches the documented export window.
pub const WINDOW_CAPACITY: usize = 512;

/// Which kind of input event ended an interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Button,
    Axis,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Axis => "axis",
        }
    }
}

/// One observed gap between consecutive input events from the same controller.
#[derive(Clone, Copy, Debug)]
pub struct IntervalSample {
    /// Monotonic time between the previous event and this event.
    pub interval: Duration,
    /// Monotonic time at which this event was processed by the input poller.
    pub at: Instant,
    /// The kind of event that ended the interval.
    pub kind: EventKind,
}

/// Fixed-capacity ring buffer; the oldest sample is dropped once full.
#[derive(Debug, Default)]
pub struct IntervalWindow {
    samples: VecDeque<IntervalSample>,
}

impl IntervalWindow {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(WINDOW_CAPACITY),
        }
    }

    pub fn push(&mut self, sample: IntervalSample) {
        if self.samples.len() == WINDOW_CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &IntervalSample> {
        self.samples.iter()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    #[cfg(test)]
    pub fn samples(&self) -> &VecDeque<IntervalSample> {
        &self.samples
    }

    pub fn summary(&self) -> Option<Summary> {
        if self.samples.is_empty() {
            return None;
        }
        let values: Vec<f64> = self
            .samples
            .iter()
            .map(|sample| sample.interval.as_secs_f64() * 1000.0)
            .collect();
        Some(summarize(&values))
    }
}

/// Distribution statistics in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    pub count: usize,
    pub min_ms: f64,
    pub mean_ms: f64,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

/// Summarize a non-empty set of millisecond values.
///
/// Percentiles use the nearest-rank method (the smallest value whose rank covers
/// the requested percentile), which is stable for small samples.
pub fn summarize(values_ms: &[f64]) -> Summary {
    assert!(!values_ms.is_empty(), "cannot summarize an empty sample");

    let mut sorted = values_ms.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));

    let count = sorted.len();
    let sum: f64 = sorted.iter().sum();
    let mean_ms = sum / count as f64;

    Summary {
        count,
        min_ms: sorted[0],
        mean_ms,
        median_ms: percentile_sorted(&sorted, 0.50),
        p95_ms: percentile_sorted(&sorted, 0.95),
        max_ms: sorted[count - 1],
    }
}

/// Nearest-rank percentile over an already sorted, non-empty slice.
pub fn percentile_sorted(sorted: &[f64], percentile: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    let percentile = percentile.clamp(0.0, 1.0);
    let rank = (percentile * sorted.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ms: u64) -> IntervalSample {
        IntervalSample {
            interval: Duration::from_millis(ms),
            at: Instant::now(),
            kind: EventKind::Button,
        }
    }

    #[test]
    fn window_keeps_only_the_latest_samples() {
        let mut window = IntervalWindow::new();
        for ms in 0..(WINDOW_CAPACITY as u64 + 10) {
            window.push(sample(ms));
        }

        assert_eq!(window.len(), WINDOW_CAPACITY);
        assert_eq!(
            window.samples().front().unwrap().interval,
            Duration::from_millis(10)
        );
        assert_eq!(
            window.samples().back().unwrap().interval,
            Duration::from_millis(WINDOW_CAPACITY as u64 + 9)
        );
    }

    #[test]
    fn summary_uses_nearest_rank_percentiles() {
        let summary = summarize(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(summary.count, 4);
        assert_eq!(summary.min_ms, 1.0);
        assert_eq!(summary.mean_ms, 2.5);
        assert_eq!(summary.median_ms, 2.0);
        assert_eq!(summary.p95_ms, 4.0);
        assert_eq!(summary.max_ms, 4.0);
    }

    #[test]
    fn empty_window_has_no_summary() {
        assert!(IntervalWindow::new().summary().is_none());
    }

    #[test]
    fn summary_of_single_sample() {
        let summary = summarize(&[16.0]);
        assert_eq!(summary.median_ms, 16.0);
        assert_eq!(summary.p95_ms, 16.0);
        assert_eq!(summary.mean_ms, 16.0);
    }
}
