use std::collections::VecDeque;
use std::time::Duration;

const MAX_INTERVALS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntervalSummary {
    pub count: usize,
    pub mean_ms: f64,
    pub p95_ms: f64,
    pub min_ms: f64,
}

#[derive(Debug, Default)]
pub struct IntervalSeries {
    values_ms: VecDeque<f64>,
}

impl IntervalSeries {
    pub fn push(&mut self, interval: Duration) {
        let value_ms = interval.as_secs_f64() * 1_000.0;
        if !value_ms.is_finite() || value_ms <= 0.0 {
            return;
        }

        if self.values_ms.len() == MAX_INTERVALS {
            self.values_ms.pop_front();
        }
        self.values_ms.push_back(value_ms);
    }

    pub fn samples(&self) -> impl Iterator<Item = f64> + '_ {
        self.values_ms.iter().copied()
    }

    pub fn summary(&self) -> Option<IntervalSummary> {
        if self.values_ms.is_empty() {
            return None;
        }

        let mut sorted: Vec<f64> = self.values_ms.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);

        let mean_ms = sorted.iter().sum::<f64>() / sorted.len() as f64;
        let p95_index = ((sorted.len() as f64 * 0.95).ceil() as usize)
            .saturating_sub(1)
            .min(sorted.len() - 1);

        Some(IntervalSummary {
            count: sorted.len(),
            mean_ms,
            p95_ms: sorted[p95_index],
            min_ms: sorted[0],
        })
    }

    pub fn clear(&mut self) {
        self.values_ms.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::IntervalSeries;
    use std::time::Duration;

    #[test]
    fn summarizes_samples() {
        let mut series = IntervalSeries::default();
        series.push(Duration::from_millis(10));
        series.push(Duration::from_millis(20));
        series.push(Duration::from_millis(30));

        let summary = series.summary().expect("samples should have a summary");
        assert_eq!(summary.count, 3);
        assert!((summary.mean_ms - 20.0).abs() < f64::EPSILON);
        assert_eq!(summary.p95_ms, 30.0);
        assert_eq!(summary.min_ms, 10.0);
    }

    #[test]
    fn ignores_zero_intervals_and_clears() {
        let mut series = IntervalSeries::default();
        series.push(Duration::ZERO);
        assert!(series.summary().is_none());

        series.push(Duration::from_millis(5));
        series.clear();
        assert!(series.summary().is_none());
    }
}
