//! Worker failure closes admission and wakes queued callers before native cleanup.

use super::{Inner, Job, Reclaim};
use crate::{
    FailurePhase, PanicReport, ThreadComponent, ThreadFailure, control::Shared, signal::lock,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Weak, atomic::Ordering},
};

struct Charge<'a> {
    inner: &'a Inner,
    discard: bool,
    wake: Option<(crate::wait::WaitRegistration, vthread_stack::ParkToken)>,
    retained: bool,
}

enum Work {
    Execute(Job),
    Reclaim(Reclaim),
}

impl Drop for Charge<'_> {
    fn drop(&mut self) {
        // Closing an already selected generation cannot replace its selected winner.
        if let Some((wake, token)) = &self.wake {
            wake.select_closed(*token);
        }
        if self.retained {
            return;
        }
        let mut state = lock(&self.inner.state);
        if self.discard {
            state.discarding -= 1;
        } else {
            state.running -= 1;
        }
    }
}

pub(super) fn run(inner: Arc<Inner>, owner: Weak<Shared>) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| work(&inner))) {
        fail(&inner, &owner, PanicReport::capture(payload));
        // Admission is now closed. At most capacity jobs need native capture cleanup.
        // Each attempt removes its job before user code runs, even if cleanup panics.
        for _ in 0..inner.capacity {
            let Some((job, charge)) = take(&inner) else {
                break;
            };
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| execute(job, charge))) {
                fail(&inner, &owner, PanicReport::capture(payload));
            }
        }
    }
}

fn work(inner: &Inner) {
    loop {
        #[cfg(test)]
        if inner
            .fail_worker
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            panic!("injected native worker failure");
        }
        let Some((job, charge)) = take(inner) else {
            return;
        };
        execute(job, charge);
    }
}

fn take(inner: &Inner) -> Option<(Work, Charge<'_>)> {
    let mut state = lock(&inner.state);
    loop {
        if let Some(index) = state
            .completed
            .iter()
            .position(|reclaim| reclaim.abandoned.load(Ordering::Acquire))
        {
            let reclaim = state
                .completed
                .remove(index)
                .expect("indexed retained result");
            state.discarding += 1;
            return Some((
                Work::Reclaim(reclaim),
                Charge {
                    inner,
                    discard: true,
                    wake: None,
                    retained: false,
                },
            ));
        }
        if let Some(job) = state.queue.pop_front() {
            let discard = state.stopped || job.reclaim.abandoned.load(Ordering::Acquire);
            if discard {
                state.discarding += 1;
            } else {
                state.running += 1;
            }
            let charge = Charge {
                inner,
                discard,
                wake: Some((job.wake.clone(), job.token)),
                retained: false,
            };
            return Some((Work::Execute(job), charge));
        }
        if state.stopped && state.completed.is_empty() {
            return None;
        }
        state = inner
            .changed
            .wait(state)
            .unwrap_or_else(|poison| poison.into_inner());
    }
}

fn execute(work: Work, mut charge: Charge<'_>) {
    let job = match work {
        Work::Execute(job) => job,
        Work::Reclaim(reclaim) => {
            let panicked = destroy(reclaim.body);
            lock(&charge.inner.state).panicked += u64::from(panicked);
            return;
        }
    };
    if charge.discard && let Some((wake, token)) = &charge.wake {
        wake.select_closed(*token);
    }
    let Job { body, reclaim, .. } = job;
    let result = catch_unwind(AssertUnwindSafe(|| {
        if charge.discard {
            drop(body);
            false
        } else {
            body()
        }
    }));
    let panicked = match result {
        Ok(panicked) => panicked,
        Err(payload) => {
            PanicReport::capture(payload);
            true
        }
    };
    lock(&charge.inner.state).panicked += u64::from(panicked);
    if charge.discard || reclaim.abandoned.load(Ordering::Acquire) {
        let panicked = destroy(reclaim.body);
        lock(&charge.inner.state).panicked += u64::from(panicked);
    } else {
        // The native pool retains a result owner until the caller commits or its
        // stack is revoked. Transfer capacity atomically from running to completed.
        let mut state = lock(&charge.inner.state);
        state.running -= 1;
        state.completed.push_back(reclaim);
        charge.retained = true;
        charge.inner.changed.notify_all();
    }
    // The charge survives all body/result/payload destruction, including unwind.
}

fn destroy(body: Box<dyn FnOnce() + Send>) -> bool {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(()) => false,
        Err(payload) => {
            PanicReport::capture(payload);
            true
        }
    }
}

fn fail(inner: &Inner, owner: &Weak<Shared>, panic: PanicReport) {
    let queued = {
        let mut state = lock(&inner.state);
        state.failed = true;
        state.stopped = true;
        inner.changed.notify_all();
        state
            .queue
            .iter()
            .map(|job| (job.wake.clone(), job.token))
            .collect::<Vec<_>>()
    };
    if let Some(owner) = owner.upgrade() {
        owner.record_failure(ThreadFailure::new(
            ThreadComponent::NativeWorker,
            std::thread::current().name().unwrap_or("native worker"),
            FailurePhase::Running,
            panic,
        ));
    }
    for (wake, token) in queued {
        wake.select_closed(token);
    }
}

#[cfg(test)]
#[path = "worker_test.rs"]
mod worker_test;
