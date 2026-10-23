use std::time::{Duration, Instant, SystemTime};

pub const DEFAULT_TRIAL_COUNT: usize = 20;
pub const RESULT_PAUSE: Duration = Duration::from_millis(1_000);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueShape {
    Circle,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactionPhase {
    Idle,
    Waiting {
        cue_at: Instant,
        shape: CueShape,
    },
    CueVisible {
        shape: CueShape,
        submitted_at: Instant,
        submitted_wall_time: SystemTime,
    },
    ResultPause {
        until: Instant,
    },
    Paused,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressOutcome {
    Ignored,
    FalseStart,
    Recorded(Duration),
    Complete(Duration),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReactionSummary {
    pub mean_ms: f64,
    pub p95_ms: f64,
    pub best_ms: f64,
}

#[derive(Debug)]
pub struct ReactionSession {
    target_trials: usize,
    phase: ReactionPhase,
    results: Vec<Duration>,
    false_starts: usize,
    last_result: Option<Duration>,
}

impl Default for ReactionSession {
    fn default() -> Self {
        Self::new(DEFAULT_TRIAL_COUNT)
    }
}

impl ReactionSession {
    pub fn new(target_trials: usize) -> Self {
        Self {
            target_trials: target_trials.max(1),
            phase: ReactionPhase::Idle,
            results: Vec::new(),
            false_starts: 0,
            last_result: None,
        }
    }

    pub fn start(&mut self, now: Instant, delay: Duration, shape: CueShape) {
        self.results.clear();
        self.false_starts = 0;
        self.last_result = None;
        self.schedule_trial(now, delay, shape);
    }

    pub fn phase(&self) -> ReactionPhase {
        self.phase
    }

    pub fn results(&self) -> &[Duration] {
        &self.results
    }

    pub fn false_starts(&self) -> usize {
        self.false_starts
    }

    pub fn last_result(&self) -> Option<Duration> {
        self.last_result
    }

    pub fn target_trials(&self) -> usize {
        self.target_trials
    }

    pub fn advance_cue(&mut self, now: Instant, wall_time: SystemTime) {
        if let ReactionPhase::Waiting { cue_at, shape } = self.phase {
            if now >= cue_at {
                self.phase = ReactionPhase::CueVisible {
                    shape,
                    submitted_at: now,
                    submitted_wall_time: wall_time,
                };
            }
        }
    }

    pub fn needs_next_trial(&self, now: Instant) -> bool {
        matches!(self.phase, ReactionPhase::ResultPause { until } if now >= until)
    }

    pub fn schedule_next_trial(&mut self, now: Instant, delay: Duration, shape: CueShape) {
        if self.needs_next_trial(now) {
            self.schedule_trial(now, delay, shape);
        }
    }

    pub fn button_pressed(
        &mut self,
        now: Instant,
        event_wall_time: SystemTime,
        restart_delay: Duration,
        restart_shape: CueShape,
    ) -> PressOutcome {
        match self.phase {
            ReactionPhase::Waiting { .. } => {
                self.false_starts += 1;
                self.schedule_trial(now, restart_delay, restart_shape);
                PressOutcome::FalseStart
            }
            ReactionPhase::CueVisible {
                submitted_at,
                submitted_wall_time,
                ..
            } => {
                if event_wall_time < submitted_wall_time {
                    self.false_starts += 1;
                    self.schedule_trial(now, restart_delay, restart_shape);
                    return PressOutcome::FalseStart;
                }

                let elapsed = now.saturating_duration_since(submitted_at);
                self.results.push(elapsed);
                self.last_result = Some(elapsed);

                if self.results.len() >= self.target_trials {
                    self.phase = ReactionPhase::Complete;
                    PressOutcome::Complete(elapsed)
                } else {
                    self.phase = ReactionPhase::ResultPause {
                        until: now + RESULT_PAUSE,
                    };
                    PressOutcome::Recorded(elapsed)
                }
            }
            ReactionPhase::Idle
            | ReactionPhase::ResultPause { .. }
            | ReactionPhase::Paused
            | ReactionPhase::Complete => PressOutcome::Ignored,
        }
    }

    pub fn pause(&mut self) {
        if matches!(
            self.phase,
            ReactionPhase::Waiting { .. }
                | ReactionPhase::CueVisible { .. }
                | ReactionPhase::ResultPause { .. }
        ) {
            self.phase = ReactionPhase::Paused;
        }
    }

    pub fn resume(&mut self, now: Instant, delay: Duration, shape: CueShape) {
        if matches!(self.phase, ReactionPhase::Paused) {
            self.schedule_trial(now, delay, shape);
        }
    }

    pub fn reset(&mut self) {
        self.phase = ReactionPhase::Idle;
        self.results.clear();
        self.false_starts = 0;
        self.last_result = None;
    }

    pub fn summary(&self) -> Option<ReactionSummary> {
        if self.results.is_empty() {
            return None;
        }

        let mut samples_ms: Vec<f64> = self
            .results
            .iter()
            .map(Duration::as_secs_f64)
            .map(|seconds| seconds * 1_000.0)
            .collect();
        samples_ms.sort_by(f64::total_cmp);
        let p95_index = ((samples_ms.len() as f64 * 0.95).ceil() as usize)
            .saturating_sub(1)
            .min(samples_ms.len() - 1);

        Some(ReactionSummary {
            mean_ms: samples_ms.iter().sum::<f64>() / samples_ms.len() as f64,
            p95_ms: samples_ms[p95_index],
            best_ms: samples_ms[0],
        })
    }

    fn schedule_trial(&mut self, now: Instant, delay: Duration, shape: CueShape) {
        self.phase = ReactionPhase::Waiting {
            cue_at: now + delay,
            shape,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{CueShape, PressOutcome, ReactionPhase, ReactionSession};
    use std::time::{Duration, Instant, SystemTime};

    #[test]
    fn false_start_restarts_the_wait_without_counting_a_result() {
        let now = Instant::now();
        let wall_time = SystemTime::UNIX_EPOCH;
        let mut session = ReactionSession::new(2);
        session.start(now, Duration::from_secs(1), CueShape::Circle);

        let outcome = session.button_pressed(
            now + Duration::from_millis(250),
            wall_time + Duration::from_millis(250),
            Duration::from_secs(2),
            CueShape::Square,
        );

        assert_eq!(outcome, PressOutcome::FalseStart);
        assert_eq!(session.false_starts(), 1);
        assert!(session.results().is_empty());
        assert_eq!(
            session.phase(),
            ReactionPhase::Waiting {
                cue_at: now + Duration::from_millis(2_250),
                shape: CueShape::Square,
            }
        );
    }

    #[test]
    fn cue_response_records_monotonic_elapsed_time() {
        let now = Instant::now();
        let wall_time = SystemTime::UNIX_EPOCH;
        let mut session = ReactionSession::new(2);
        session.start(now, Duration::ZERO, CueShape::Circle);
        session.advance_cue(now, wall_time);

        let outcome = session.button_pressed(
            now + Duration::from_millis(240),
            wall_time + Duration::from_millis(240),
            Duration::from_secs(2),
            CueShape::Square,
        );

        assert_eq!(outcome, PressOutcome::Recorded(Duration::from_millis(240)));
        assert_eq!(session.results(), &[Duration::from_millis(240)]);
        assert_eq!(session.last_result(), Some(Duration::from_millis(240)));
    }

    #[test]
    fn stale_queued_press_does_not_count_as_a_reaction() {
        let now = Instant::now();
        let wall_time = SystemTime::UNIX_EPOCH;
        let mut session = ReactionSession::new(2);
        session.start(now, Duration::ZERO, CueShape::Circle);
        session.advance_cue(now, wall_time + Duration::from_secs(1));

        let outcome = session.button_pressed(
            now + Duration::from_millis(50),
            wall_time + Duration::from_millis(900),
            Duration::from_secs(2),
            CueShape::Square,
        );

        assert_eq!(outcome, PressOutcome::FalseStart);
        assert_eq!(session.false_starts(), 1);
        assert!(session.results().is_empty());
    }

    #[test]
    fn session_completes_and_reset_clears_results() {
        let now = Instant::now();
        let wall_time = SystemTime::UNIX_EPOCH;
        let mut session = ReactionSession::new(1);
        session.start(now, Duration::ZERO, CueShape::Square);
        session.advance_cue(now, wall_time);
        let outcome = session.button_pressed(
            now + Duration::from_millis(80),
            wall_time + Duration::from_millis(80),
            Duration::ZERO,
            CueShape::Circle,
        );

        assert_eq!(outcome, PressOutcome::Complete(Duration::from_millis(80)));
        assert_eq!(session.phase(), ReactionPhase::Complete);
        assert_eq!(session.summary().expect("one sample").best_ms, 80.0);

        session.reset();
        assert_eq!(session.phase(), ReactionPhase::Idle);
        assert!(session.results().is_empty());
        assert_eq!(session.false_starts(), 0);
        assert!(session.summary().is_none());
    }

    #[test]
    fn pause_and_resume_void_the_active_cue_but_keep_finished_trials() {
        let now = Instant::now();
        let wall_time = SystemTime::UNIX_EPOCH;
        let mut session = ReactionSession::new(2);
        session.start(now, Duration::ZERO, CueShape::Circle);
        session.advance_cue(now, wall_time);
        assert_eq!(
            session.button_pressed(
                now + Duration::from_millis(120),
                wall_time + Duration::from_millis(120),
                Duration::ZERO,
                CueShape::Square,
            ),
            PressOutcome::Recorded(Duration::from_millis(120))
        );

        session.pause();
        assert_eq!(session.phase(), ReactionPhase::Paused);

        let resumed_at = now + Duration::from_secs(30);
        session.resume(resumed_at, Duration::from_secs(2), CueShape::Square);
        assert_eq!(session.results(), &[Duration::from_millis(120)]);
        assert_eq!(
            session.phase(),
            ReactionPhase::Waiting {
                cue_at: resumed_at + Duration::from_secs(2),
                shape: CueShape::Square,
            }
        );
    }
}
