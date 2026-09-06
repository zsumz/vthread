//! Leaf owner-local updates; no borrow or profile reference crosses a suspension.

use crate::handoff_profile::{HandoffProfile, HandoffStage};
use std::time::{Duration, Instant};

pub(crate) fn record(update: impl FnOnce(&mut HandoffProfile)) {
    crate::context::wake::with_handoff_profile(update);
}

pub(crate) fn duration(stage: HandoffStage, elapsed: Duration) {
    record(|profile| profile.record(stage, elapsed));
}

pub(crate) struct Span {
    stage: HandoffStage,
    started: Instant,
}

impl Span {
    pub(crate) fn new(stage: HandoffStage) -> Self {
        Self {
            stage,
            started: Instant::now(),
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        duration(self.stage, self.started.elapsed());
    }
}

#[cfg(test)]
#[path = "handoff_span_test.rs"]
mod handoff_span_test;
