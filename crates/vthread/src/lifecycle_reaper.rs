//! Contained reaping; an interrupted claim returns ownership to the process table.

use super::{Entry, State};
use crate::{
    FailurePhase, PanicReport, ShutdownPhase, ThreadComponent, ThreadFailure, signal::lock,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::{Duration, Instant},
};

struct Claim {
    state: Arc<State>,
    entry: Option<Entry>,
}

impl Claim {
    fn ready(state: &Arc<State>) -> Option<Self> {
        let mut slots = lock(&state.slots);
        let index = slots
            .entries
            .iter()
            .position(|entry| entry.worker.finished())?;
        Some(Self {
            state: Arc::clone(state),
            entry: Some(slots.entries.swap_remove(index)),
        })
    }

    fn join(mut self) -> Option<ThreadFailure> {
        #[cfg(test)]
        inject(&self.state, 2);
        let entry = self.entry.as_mut().expect("claimed entry");
        entry
            .worker
            .join(&Arc::downgrade(&entry.shared), ThreadComponent::Coordinator);
        #[cfg(test)]
        inject(&self.state, 3);
        if !entry
            .resources
            .drained(&entry.shared, entry.worker.joined())
        {
            // Retain all unjoined carrier/service handles; dropping Shared could
            // otherwise block the process owner in live service cleanup.
            return Some(failure("coordinator exited before cleanup was confirmed"));
        }
        let phase = if lock(&entry.shared.failures).is_empty() {
            ShutdownPhase::Complete
        } else {
            ShutdownPhase::Failed
        };
        let shared = Arc::clone(&entry.shared);
        #[cfg(test)]
        inject(&self.state, 4);
        // All handles and affine state are already reclaimed. Drop the inert entry
        // and release admission before making terminal shutdown observable.
        drop(self.entry.take());
        lock(&self.state.slots).occupied -= 1;
        #[cfg(test)]
        if let Some(hook) = lock(&self.state.terminal_hook).take() {
            hook();
        }
        drop(self);
        // Final commit: no entry cleanup, capacity accounting, or failure reporting
        // follows publication. The remaining Shared reference contains inert state.
        shared.advance_shutdown(phase);
        None
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if let Some(entry) = self.entry.take() {
            lock(&self.state.slots).entries.push(entry);
        }
    }
}

pub(super) fn run(state: Arc<State>) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        crate::worker_context::enter();
        reap(&state)
    }));
    let failure = match result {
        Ok(failure) => failure,
        Err(payload) => {
            let (captured, _) = vthread_stack::panic_payload::capture_for_join(payload);
            ThreadFailure::new(
                ThreadComponent::LifecycleOwner,
                "vthread-lifecycle-owner",
                FailurePhase::Running,
                PanicReport::from_captured(captured),
            )
        }
    };
    state.fail(failure);
}

pub(super) fn stopped_failure() -> ThreadFailure {
    failure("lifecycle owner stopped unexpectedly")
}

fn failure(message: &'static str) -> ThreadFailure {
    let (captured, _) = vthread_stack::panic_payload::capture_for_join(Box::new(message));
    ThreadFailure::new(
        ThreadComponent::LifecycleOwner,
        "vthread-lifecycle-owner",
        FailurePhase::Running,
        PanicReport::from_captured(captured),
    )
}

fn reap(state: &Arc<State>) -> ThreadFailure {
    loop {
        let observed = state.changed.version();
        #[cfg(test)]
        inject(state, 1);
        while let Some(claim) = Claim::ready(state) {
            if let Some(failure) = claim.join() {
                return failure;
            }
        }
        let pending = !lock(&state.slots).entries.is_empty();
        let deadline = pending.then(|| Instant::now() + Duration::from_millis(10));
        state.changed.wait(observed, deadline);
    }
}

#[cfg(test)]
fn inject(state: &State, phase: usize) {
    if state.fail_at.load(std::sync::atomic::Ordering::Acquire) == phase {
        panic!("injected lifecycle owner failure at phase {phase}");
    }
}

#[cfg(test)]
#[path = "lifecycle_reaper_test.rs"]
mod lifecycle_reaper_test;
