//! Managed threads must never synchronously wait for runtime teardown.

use crate::{FailurePhase, PanicReport, ThreadComponent, ThreadFailure, control::Shared};
use std::{
    cell::{Cell, RefCell},
    sync::Weak,
};

thread_local! {
    static MANAGED: Cell<bool> = const { Cell::new(false) };
    static OWNER: RefCell<Option<(Weak<Shared>, ThreadComponent)>> = const { RefCell::new(None) };
    #[cfg(feature = "runtime-evidence")]
    static CARRIER: Cell<Option<crate::CarrierId>> = const { Cell::new(None) };
}

pub(crate) fn enter() {
    MANAGED.with(|managed| managed.set(true));
}

pub(crate) fn attach(shared: Weak<Shared>, component: ThreadComponent) {
    enter();
    OWNER.with(|owner| *owner.borrow_mut() = Some((shared, component)));
    vthread_stack::panic_payload::set_cleanup_observer(|captured| {
        payload_failure(PanicReport::from_captured(captured.clone()));
    });
}

pub(crate) fn payload_failure(panic: PanicReport) {
    let owner = OWNER
        .try_with(|owner| owner.borrow().clone())
        .ok()
        .flatten();
    if let Some((shared, component)) = owner
        && let Some(shared) = shared.upgrade()
    {
        shared.record_failure(ThreadFailure::new(
            component,
            std::thread::current().name().unwrap_or("unnamed"),
            FailurePhase::PanicCleanup,
            panic,
        ));
        shared.request_stop();
    }
}

pub(crate) fn is_managed() -> bool {
    // During OS TLS destruction it is too late to safely start a blocking teardown.
    MANAGED.try_with(Cell::get).unwrap_or(true)
}

#[cfg(feature = "runtime-evidence")]
pub(crate) fn set_carrier(id: crate::CarrierId) {
    CARRIER.with(|carrier| carrier.set(Some(id)));
}

#[cfg(feature = "runtime-evidence")]
pub(crate) fn current_carrier() -> Option<crate::CarrierId> {
    CARRIER.try_with(Cell::get).ok().flatten()
}

#[cfg(test)]
#[path = "worker_context_test.rs"]
mod worker_context_test;
