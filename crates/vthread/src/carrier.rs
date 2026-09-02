//! Persistent OS carrier lifecycle and scheduler-fault containment.

use crate::{CarrierId, CarrierStatus, Result, TaskFailure, control::Shared, kernel::Kernel};
use std::{
    mem::ManuallyDrop,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, atomic::Ordering},
};

pub(crate) fn run(shared: Arc<Shared>, id: CarrierId) {
    #[cfg(feature = "runtime-evidence")]
    crate::worker_context::set_carrier(id);
    // A cleanup fault must retain affine stacks rather than run fallible field drops
    // during unwinding. Such stacks are never resumed and remain allocated until exit.
    let mut kernel = ManuallyDrop::new(Box::new(Kernel::new(Arc::clone(&shared), id)));
    let result = catch_unwind(AssertUnwindSafe(|| finish(&mut kernel, &shared)));
    shared.inboxes[id.0]
        .scheduler_stopped
        .store(true, Ordering::Release);
    match result {
        Ok(()) if kernel.reclaimed() => {
            drop(ManuallyDrop::into_inner(kernel));
            shared.inboxes[id.0]
                .reclaimed
                .store(true, Ordering::Release);
        }
        outcome => {
            let panic = match outcome {
                Err(payload) => crate::PanicReport::capture(payload),
                _ => crate::PanicReport::capture(Box::new("carrier cleanup incomplete")),
            };
            record_failure(&shared, panic);
            kernel.publish(CarrierStatus::Failed);
            shared.request_stop();
        }
    }
}

fn finish(kernel: &mut Kernel, shared: &Shared) {
    let outcome = catch_unwind(AssertUnwindSafe(|| drive(kernel)));
    if !matches!(outcome, Ok(Ok(()))) {
        let panic = match outcome {
            Err(payload) => crate::PanicReport::capture(payload),
            Ok(result) => {
                crate::PanicReport::capture(Box::new(result.expect_err("failed drive").to_string()))
            }
        };
        record_failure(shared, panic);
        kernel.inbox.stop();
        kernel.abort(None, TaskFailure::CarrierFailed);
        kernel.retire(CarrierStatus::Failed);
    } else {
        kernel.retire(CarrierStatus::Stopped);
    }
}

fn record_failure(shared: &Shared, panic: crate::PanicReport) {
    shared.record_failure(crate::ThreadFailure::new(
        crate::ThreadComponent::Carrier,
        std::thread::current().name().unwrap_or("carrier"),
        crate::FailurePhase::Running,
        panic,
    ));
}

fn drive(kernel: &mut Kernel) -> Result<()> {
    let mut handled = None;
    // One empty-to-nonempty signal covers every bounded receive batch until drained.
    let mut remote_pending = false;
    loop {
        let observed = kernel.inbox.signal.version();
        let signal_changed = handled != Some(observed);
        if signal_changed {
            if kernel.inbox.stopped() {
                kernel.abort(None, TaskFailure::RuntimeStopped);
                return Ok(());
            }
            while let Some((scope, reason)) = kernel.inbox.take_abort() {
                kernel.abort(Some(scope), reason);
            }
            remote_pending = kernel.receive();
            handled = Some(observed);
        } else if remote_pending {
            remote_pending = kernel.receive();
        } else {
            kernel.receive_local();
        }
        if !kernel.tick(signal_changed)? {
            kernel.wait_for_work(observed);
        }
    }
}

#[cfg(test)]
#[path = "carrier_test.rs"]
mod carrier_test;
