//! One allocation owns a transferable entry and its typed join result.

use crate::{PanicReport, join::JoinOutcome, signal::lock, task::SharedTaskRecord};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

trait TaskEntry: Send + Sync {
    fn run(self: Arc<Self>, record: &SharedTaskRecord);
    fn discard(self: Arc<Self>);
}

struct EnvelopeState<T, F> {
    entry: Option<F>,
    outcome: Option<std::result::Result<T, PanicReport>>,
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
        state: Mutex::new(EnvelopeState {
            entry: Some(entry),
            outcome: None,
        }),
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
                let entry = state.entry.expect("unstarted task entry");
                run(record, entry, drop);
                return;
            }
            Err(envelope) => envelope,
        };
        let entry = lock(&envelope.state)
            .entry
            .take()
            .expect("unstarted task entry");
        run(record, entry, |outcome| {
            lock(&envelope.state).outcome = Some(outcome);
        });
    }

    fn discard(self: Arc<Self>) {
        let entry = match Arc::try_unwrap(self) {
            Ok(envelope) => {
                envelope
                    .state
                    .into_inner()
                    .unwrap_or_else(|error| error.into_inner())
                    .entry
            }
            Err(envelope) => lock(&envelope.state).entry.take(),
        };
        drop(entry);
    }
}

impl<T: Send, F: Send> JoinOutcome<T> for Envelope<T, F> {
    fn take(&self) -> Option<std::result::Result<T, PanicReport>> {
        lock(&self.state).outcome.take()
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
