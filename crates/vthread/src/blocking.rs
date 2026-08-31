//! Explicit delegation of owned native work to a bounded runtime worker pool.
//!
//! Cancellation leaves queued jobs as native-owned tombstones. Already-running
//! calls cannot be stopped: the caller stops waiting, while the runtime retains
//! the job and reclaims its captures and result on a native worker. Capacity stays
//! charged until this cleanup finishes. Runtime shutdown waits for those calls;
//! scope exit alone does not drain abandoned native calls. Closures/results must
//! be Send + 'static. Pool saturation rejects work immediately, before ownership
//! transfers: the consumed closure and captures are destroyed on the calling thread,
//! not returned to the user. Their destructors can block or panic on that carrier.
//! A result whose Ready wake wins is committed when the caller takes
//! it; later cancellation is observed at the next cooperative boundary.

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
/// Successful submission transfers capture/result cleanup to native workers. On
/// rejection, this consumed closure and its captures are destroyed on the calling
/// thread and are not recoverable; their destructors can block or panic there.
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
    let reclaim_output = Arc::clone(&output);
    let abandoned = Arc::new(AtomicBool::new(false));
    let worker_abandoned = Arc::clone(&abandoned);
    let parker = Parker {
        wait: WaitCell::new(),
    };
    let mut lease = None;
    let outcome = parker.park_registered(|token, wake| {
        let completion = wake.clone();
        lease = Some(services.blocking.submit(
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
            Box::new(move || reclaim_output.discard()),
        )?);
        Ok(())
    })?;
    if outcome == ParkOutcome::Closed {
        return Err(if services.blocking.is_failed() {
            Error::BlockingFailed
        } else if services.blocking.is_stopped() {
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
