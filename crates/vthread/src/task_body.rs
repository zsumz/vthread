//! One allocation owns a transferable entry and its typed join result.

use crate::{PanicReport, join::JoinOutcome, signal::lock, task::SharedTaskRecord};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

trait TaskEntry: Send + Sync {
    fn run(self: Arc<Self>, record: &SharedTaskRecord);
    fn discard(self: Arc<Self>);
}

enum EnvelopeState<T, F> {
    Pending(F),
    Complete(std::result::Result<T, PanicReport>),
    Empty,
}

struct Envelope<T, F> {
    state: Mutex<EnvelopeState<T, F>>,
}

pub(crate) struct TaskStart(Option<Arc<dyn TaskEntry>>);

pub(crate) fn transferable<T, F>(entry: F) -> (TaskStart, Arc<dyn JoinOutcome<T>>)
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let envelope = Arc::new(Envelope {
        state: Mutex::new(EnvelopeState::Pending(entry)),
    });
    let start: Arc<dyn TaskEntry> = envelope.clone();
    let outcome: Arc<dyn JoinOutcome<T>> = envelope;
    (TaskStart(Some(start)), outcome)
}

impl TaskStart {
    pub(crate) fn run(mut self, record: &SharedTaskRecord) {
        let entry = self.0.take().expect("unstarted task entry");
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| entry.run(record))) {
            record.lock().panic = Some(PanicReport::capture(payload));
        }
    }
}

impl Drop for TaskStart {
    fn drop(&mut self) {
        if let Some(entry) = self.0.take() {
            entry.discard();
        }
    }
}

impl<T, F> TaskEntry for Envelope<T, F>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    fn run(self: Arc<Self>, record: &SharedTaskRecord) {
        let envelope = match Arc::try_unwrap(self) {
            Ok(envelope) => {
                let state = envelope
                    .state
                    .into_inner()
                    .unwrap_or_else(|error| error.into_inner());
                let EnvelopeState::Pending(entry) = state else {
                    panic!("unstarted task entry");
                };
                run(record, entry, drop);
                return;
            }
            Err(envelope) => envelope,
        };
        let entry = take_entry(&envelope.state);
        run(record, entry, |outcome| {
            *lock(&envelope.state) = EnvelopeState::Complete(outcome);
        });
    }

    fn discard(self: Arc<Self>) {
        let entry = match Arc::try_unwrap(self) {
            Ok(envelope) => {
                let state = envelope
                    .state
                    .into_inner()
                    .unwrap_or_else(|error| error.into_inner());
                match state {
                    EnvelopeState::Pending(entry) => Some(entry),
                    EnvelopeState::Complete(_) | EnvelopeState::Empty => None,
                }
            }
            Err(envelope) => Some(take_entry(&envelope.state)),
        };
        drop(entry);
    }
}

impl<T: Send, F: Send> JoinOutcome<T> for Envelope<T, F> {
    fn take(&self) -> Option<std::result::Result<T, PanicReport>> {
        let mut state = lock(&self.state);
        let previous = std::mem::replace(&mut *state, EnvelopeState::Empty);
        match previous {
            EnvelopeState::Complete(outcome) => Some(outcome),
            previous => {
                *state = previous;
                None
            }
        }
    }
}

fn take_entry<T, F>(state: &Mutex<EnvelopeState<T, F>>) -> F {
    let mut state = lock(state);
    let previous = std::mem::replace(&mut *state, EnvelopeState::Empty);
    match previous {
        EnvelopeState::Pending(entry) => entry,
        previous => {
            *state = previous;
            panic!("unstarted task entry");
        }
    }
}

pub(crate) fn run<T>(
    record: &SharedTaskRecord,
    entry: impl FnOnce() -> T,
    store: impl FnOnce(std::result::Result<T, PanicReport>),
) {
    let outcome = catch_unwind(AssertUnwindSafe(entry)).map_err(PanicReport::capture);
    if let Err(panic) = &outcome {
        record.lock().panic = Some(panic.clone());
    }
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| store(outcome))) {
        record.lock().panic = Some(PanicReport::capture(payload));
    }
}

#[cfg(test)]
#[path = "task_body_test.rs"]
mod task_body_test;
