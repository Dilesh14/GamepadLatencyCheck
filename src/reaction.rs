//! Visual reaction test state machine.
//!
//! The measured value is app-observed reaction time: from the moment the UI
//! submits the cue for drawing to the moment the input poller receives a button
//! press. It includes human response and app scheduling, and is not controller
//! hardware latency or photon-to-input latency.

use std::time::{Duration, Instant};

use crate::stats::{Summary, summarize};

pub const TARGET_TRIALS: usize = 20;

const MIN_CUE_DELAY_MS: u64 = 1_500;
const MAX_CUE_DELAY_MS: u64 = 4_000;
const FALSE_START_RESTART_MS: u64 = 900;
const INTER_TRIAL_MS: u64 = 650;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueShape {
    Circle,
    Square,
}

impl CueShape {
    fn sample() -> Self {
        if fastrand::bool() {
            Self::Circle
        } else {
            Self::Square
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// No session has been started.
    Idle,
    /// A cue is scheduled but not visible yet.
    Waiting,
    /// The cue is visible and a press is expected.
    Cue,
    /// The user pressed before the cue; the trial will restart.
    FalseStart,
    /// A successful trial was recorded; the next one is scheduled shortly.
    InterTrial,
    /// The session is paused, typically because the controller went away.
    Paused,
    /// All target trials have been completed.
    Complete,
}

impl Phase {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Waiting | Self::Cue | Self::FalseStart | Self::InterTrial
        )
    }
}

#[derive(Clone, Debug)]
pub struct TrialResult {
    pub shape: CueShape,
    pub reaction: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressOutcome {
    /// A trial was completed.
    Success,
    /// The press came before the cue.
    FalseStart,
    /// The press was ignored because no trial was accepting input.
    Ignored,
}

pub struct ReactionSession {
    phase: Phase,
    trials: Vec<TrialResult>,
    false_starts: u32,
    controller_name: Option<String>,
    cue_shape: CueShape,
    cue_shown_at: Option<Instant>,
    /// Time of the next automatic transition (cue or next trial).
    deadline: Option<Instant>,
    pause_reason: Option<String>,
}

impl Default for ReactionSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ReactionSession {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            trials: Vec::with_capacity(TARGET_TRIALS),
            false_starts: 0,
            controller_name: None,
            cue_shape: CueShape::Circle,
            cue_shown_at: None,
            deadline: None,
            pause_reason: None,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn trials(&self) -> &[TrialResult] {
        &self.trials
    }

    pub fn false_starts(&self) -> u32 {
        self.false_starts
    }

    pub fn controller_name(&self) -> Option<&str> {
        self.controller_name.as_deref()
    }

    pub fn pause_reason(&self) -> Option<&str> {
        self.pause_reason.as_deref()
    }

    pub fn is_active(&self) -> bool {
        self.phase.is_active()
    }

    pub fn is_paused(&self) -> bool {
        self.phase == Phase::Paused
    }

    pub fn is_complete(&self) -> bool {
        self.phase == Phase::Complete
    }

    pub fn cue_shape(&self) -> CueShape {
        self.cue_shape
    }

    pub fn target(&self) -> usize {
        TARGET_TRIALS
    }

    /// The number of the trial currently being attempted, capped at the target.
    pub fn current_trial_number(&self) -> usize {
        (self.trials.len() + 1).min(TARGET_TRIALS)
    }

    pub fn stats(&self) -> Option<Summary> {
        if self.trials.is_empty() {
            return None;
        }
        let values: Vec<f64> = self
            .trials
            .iter()
            .map(|trial| trial.reaction.as_secs_f64() * 1000.0)
            .collect();
        Some(summarize(&values))
    }

    /// Start a new session and schedule the first cue.
    pub fn start(&mut self, controller_name: &str, now: Instant) {
        self.trials.clear();
        self.false_starts = 0;
        self.controller_name = Some(controller_name.to_owned());
        self.pause_reason = None;
        self.schedule_trial(now);
    }

    /// Reset the session back to its initial, not-started state.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Advance time-based transitions. Returns true if the phase changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }

        match self.phase {
            Phase::Waiting => {
                self.phase = Phase::Cue;
                self.cue_shown_at = Some(now);
                self.deadline = None;
                true
            }
            Phase::FalseStart | Phase::InterTrial => {
                self.schedule_trial(now);
                true
            }
            _ => false,
        }
    }

    /// Record a button press received at `at`.
    pub fn press(&mut self, at: Instant) -> PressOutcome {
        match self.phase {
            Phase::Waiting => self.register_false_start(at),
            Phase::Cue => {
                let Some(shown_at) = self.cue_shown_at else {
                    return PressOutcome::Ignored;
                };
                // Defensive: a queued press may carry a timestamp from before the
                // cue was submitted (for example when the app was busy). Treat it
                // as a false start instead of producing a bogus negative time.
                let Some(reaction) = at.checked_duration_since(shown_at) else {
                    return self.register_false_start(at);
                };

                self.trials.push(TrialResult {
                    shape: self.cue_shape,
                    reaction,
                });
                self.cue_shown_at = None;

                if self.trials.len() >= TARGET_TRIALS {
                    self.phase = Phase::Complete;
                    self.deadline = None;
                } else {
                    self.phase = Phase::InterTrial;
                    self.deadline = Some(at + Duration::from_millis(INTER_TRIAL_MS));
                }
                PressOutcome::Success
            }
            _ => PressOutcome::Ignored,
        }
    }

    /// Pause an active session. Idle and completed sessions are unaffected.
    pub fn pause(&mut self, reason: &str) {
        if self.phase.is_active() {
            self.phase = Phase::Paused;
            self.deadline = None;
            self.cue_shown_at = None;
            self.pause_reason = Some(reason.to_owned());
        }
    }

    /// Resume a paused session and restart the current trial with a fresh delay.
    pub fn resume(&mut self, controller_name: &str, now: Instant) {
        if self.phase == Phase::Paused {
            self.controller_name = Some(controller_name.to_owned());
            self.pause_reason = None;
            self.schedule_trial(now);
        }
    }

    fn schedule_trial(&mut self, now: Instant) {
        let delay = fastrand::u64(MIN_CUE_DELAY_MS..=MAX_CUE_DELAY_MS);
        self.phase = Phase::Waiting;
        self.cue_shape = CueShape::sample();
        self.cue_shown_at = None;
        self.deadline = Some(now + Duration::from_millis(delay));
    }

    fn register_false_start(&mut self, now: Instant) -> PressOutcome {
        self.false_starts += 1;
        self.phase = Phase::FalseStart;
        self.cue_shown_at = None;
        self.deadline = Some(now + Duration::from_millis(FALSE_START_RESTART_MS));
        PressOutcome::FalseStart
    }

    #[cfg(test)]
    fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed_session() -> (ReactionSession, Instant) {
        let mut session = ReactionSession::new();
        let mut now = Instant::now();
        session.start("Test pad", now);

        for _ in 0..TARGET_TRIALS {
            now += Duration::from_secs(5);
            assert!(session.tick(now));
            assert_eq!(session.phase(), Phase::Cue);
            now += Duration::from_millis(250);
            assert_eq!(session.press(now), PressOutcome::Success);
            now += Duration::from_secs(1);
            session.tick(now);
        }

        (session, now)
    }

    #[test]
    fn cue_delay_is_within_documented_range() {
        let mut session = ReactionSession::new();
        let now = Instant::now();
        session.start("Test pad", now);

        assert_eq!(session.phase(), Phase::Waiting);
        let delay = session.deadline().unwrap().duration_since(now);
        assert!(delay >= Duration::from_millis(MIN_CUE_DELAY_MS));
        assert!(delay <= Duration::from_millis(MAX_CUE_DELAY_MS));
    }

    #[test]
    fn pressing_before_the_cue_is_a_false_start() {
        let mut session = ReactionSession::new();
        let now = Instant::now();
        session.start("Test pad", now);

        assert_eq!(
            session.press(now + Duration::from_millis(100)),
            PressOutcome::FalseStart
        );
        assert_eq!(session.phase(), Phase::FalseStart);
        assert_eq!(session.false_starts(), 1);
        assert!(session.trials().is_empty());
    }

    #[test]
    fn successful_trial_records_reaction_time() {
        let mut session = ReactionSession::new();
        let now = Instant::now();
        session.start("Test pad", now);

        session.tick(now + Duration::from_secs(5));
        assert_eq!(session.phase(), Phase::Cue);

        assert_eq!(
            session.press(now + Duration::from_secs(5) + Duration::from_millis(212)),
            PressOutcome::Success
        );
        assert_eq!(session.trials().len(), 1);
        assert_eq!(session.trials()[0].reaction, Duration::from_millis(212));
        assert_eq!(session.phase(), Phase::InterTrial);
    }

    #[test]
    fn stray_press_during_inter_trial_is_ignored() {
        let mut session = ReactionSession::new();
        let now = Instant::now();
        session.start("Test pad", now);
        session.tick(now + Duration::from_secs(5));
        session.press(now + Duration::from_secs(5) + Duration::from_millis(200));

        assert_eq!(session.phase(), Phase::InterTrial);
        assert_eq!(
            session.press(now + Duration::from_secs(5) + Duration::from_millis(300)),
            PressOutcome::Ignored
        );
        assert_eq!(session.false_starts(), 0);
        assert_eq!(session.trials().len(), 1);
    }

    #[test]
    fn session_completes_after_twenty_successful_trials() {
        let (session, _) = completed_session();
        assert_eq!(session.phase(), Phase::Complete);
        assert_eq!(session.trials().len(), TARGET_TRIALS);
        assert!(!session.is_active());
    }

    #[test]
    fn pause_and_resume_restart_the_current_trial() {
        let mut session = ReactionSession::new();
        let now = Instant::now();
        session.start("Test pad", now);
        session.tick(now + Duration::from_secs(5));
        assert_eq!(session.phase(), Phase::Cue);

        session.pause("controller disconnected");
        assert!(session.is_paused());
        assert_eq!(session.pause_reason(), Some("controller disconnected"));
        assert_eq!(
            session.press(now + Duration::from_secs(6)),
            PressOutcome::Ignored
        );

        session.resume("Test pad", now + Duration::from_secs(7));
        assert_eq!(session.phase(), Phase::Waiting);
        assert!(session.pause_reason().is_none());
        assert!(session.trials().is_empty());
    }

    #[test]
    fn completed_session_is_not_paused() {
        let (mut session, _) = completed_session();
        session.pause("controller disconnected");
        assert_eq!(session.phase(), Phase::Complete);
    }
}
