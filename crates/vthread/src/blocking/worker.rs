//! Worker failure closes admission and wakes queued callers before native cleanup.

use super::{Inner, Job};
use crate::{
    FailurePhase, PanicReport, ThreadComponent, ThreadFailure, control::Shared, signal::lock,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Weak},
};

struct Charge<'a> {
    inner: &'a Inner,
    discard: bool,
    wake: crate::wait::WaitRegistration,
    token: vthread_stack::ParkToken,
}

impl Drop for Charge<'_> {
    fn drop(&mut self) {
        // Closing an already selected generation cannot replace its selected winner.
        self.wake.select_closed(self.token);
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

fn take(inner: &Inner) -> Option<(Job, Charge<'_>)> {
    let mut state = lock(&inner.state);
    while state.queue.is_empty() && !state.stopped {
        state = inner
            .changed
            .wait(state)
            .unwrap_or_else(|poison| poison.into_inner());
    }
    let job = state.queue.pop_front()?;
    let discard = state.stopped;
    if discard {
        state.discarding += 1;
    } else {
        state.running += 1;
    }
    let charge = Charge {
        inner,
        discard,
        wake: job.wake.clone(),
        token: job.token,
    };
    Some((job, charge))
}

fn execute(job: Job, charge: Charge<'_>) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if charge.discard {
            job.wake.select_closed(job.token);
            drop(job.body);
            false
        } else {
            (job.body)()
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
    // Capacity is charged through body/result/payload destruction, then released by RAII.
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
