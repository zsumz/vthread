//! Explicit delegation of owned native work to a bounded runtime worker pool.
//!
//! Cancellation removes queued work. Already-running calls cannot be stopped:
//! the caller stops waiting, while the runtime retains the job and drops its late
//! result. Runtime shutdown waits for those calls. Scope exit alone does not drain
//! abandoned native calls. Closures/results must be Send + 'static; borrowed work
//! cannot outlive a cancelling caller. Pool saturation rejects work immediately. Once the
//! pool is stopping, queued captures are discarded by native workers, never the stop caller.

pub(crate) mod pool;
mod result;

use crate::{
    Error, ParkOutcome, Parker, Result, SuspensionReason, context, sync::wait::Wait, wait::WaitCell,
};
use result::Output;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Runs owned native work off the carrier, parking the virtual caller for its result.
///
/// ```compile_fail
/// let local = String::from("borrowed");
/// vthread::blocking::run(|| local.len());
/// ```
pub fn run<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    run_for(SuspensionReason::Blocking, body)
}

pub(crate) fn run_for<T: Send + 'static>(
    reason: SuspensionReason,
    body: impl FnOnce() -> T + Send + 'static,
) -> Result<T> {
    let _reason = Wait::enter(reason)?;
    let mounted = context::current().ok_or(Error::OutsideVThread)?;
    let execution = mounted.execution()?;
    let services = execution
        .shared
        .services
        .get()
        .ok_or(Error::RuntimeStopped)?;
    let output = Arc::new(Output::new());
    let worker_output = Arc::clone(&output);
    let abandoned = Arc::new(AtomicBool::new(false));
    let worker_abandoned = Arc::clone(&abandoned);
    let parker = Parker {
        wait: WaitCell::new(),
    };
    let outcome = parker.park_registered(|token, wake| {
        let completion = wake.clone();
        services.blocking.submit(
            abandoned,
            token,
            wake,
            Box::new(move || {
                if worker_abandoned.load(Ordering::Acquire) {
                    return false;
                }
                let result = catch_unwind(AssertUnwindSafe(body)).map_err(|payload| {
                    Error::BlockingPanicked(crate::PanicReport::capture(payload))
                });
                let panicked = result.is_err();
                worker_output.store(result);
                completion.select_ready(token);
                panicked
            }),
        )
    })?;
    execution.data.check()?;
    if outcome == ParkOutcome::Closed {
        return Err(if services.blocking.is_stopped() {
            Error::RuntimeStopped
        } else {
            Error::BlockingPanicked(crate::PanicReport::capture(Box::new(
                "native worker cleanup failed",
            )))
        });
    }
    output.take()
}

#[cfg(test)]
#[path = "blocking_test.rs"]
mod blocking_test;
