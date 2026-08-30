//! Persistent OS carrier lifecycle and scheduler-fault containment.

use crate::{CarrierId, CarrierStatus, Result, TaskFailure, control::Shared, kernel::Kernel};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

pub(crate) fn run(shared: Arc<Shared>, id: CarrierId) {
    let mut kernel = Kernel::new(shared, id);
    let outcome = catch_unwind(AssertUnwindSafe(|| drive(&mut kernel)));
    if !matches!(outcome, Ok(Ok(()))) {
        kernel.inbox.stop();
        kernel.abort(None, TaskFailure::CarrierFailed);
        kernel.retire(CarrierStatus::Failed);
    } else {
        kernel.retire(CarrierStatus::Stopped);
    }
}

fn drive(kernel: &mut Kernel) -> Result<()> {
    loop {
        let observed = kernel.inbox.signal.version();
        if kernel.inbox.stopped() {
            kernel.abort(None, TaskFailure::RuntimeStopped);
            return Ok(());
        }
        if let Some((scope, reason)) = kernel.inbox.take_abort() {
            kernel.abort(Some(scope), reason);
        }
        kernel.receive();
        if !kernel.tick()? {
            kernel.wait_for_work(observed);
        }
    }
}

#[cfg(test)]
#[path = "carrier_test.rs"]
mod carrier_test;
