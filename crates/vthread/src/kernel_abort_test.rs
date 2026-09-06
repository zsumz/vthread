use crate::{CarrierId, TaskFailure, control::Shared, kernel::Kernel};
use std::sync::Arc;

#[test]
fn deferred_cleanup_is_coalesced_and_global_stop_dominates() {
    let mut kernel = Kernel::new(
        Arc::new(Shared::new(crate::RuntimeConfig::default())),
        CarrierId(0),
    );
    kernel.defer_abort(Some(1), TaskFailure::ScopeStalled);
    kernel.defer_abort(Some(1), TaskFailure::ScopeClosed);
    assert_eq!(kernel.pending_aborts, [(Some(1), TaskFailure::ScopeClosed)]);
    kernel.defer_abort(Some(2), TaskFailure::ScopeStalled);
    assert_eq!(kernel.pending_aborts.len(), 2);
    kernel.defer_abort(None, TaskFailure::RuntimeStopped);
    kernel.defer_abort(Some(3), TaskFailure::ScopeClosed);
    assert_eq!(kernel.pending_aborts, [(None, TaskFailure::RuntimeStopped)]);
    kernel.retry_aborts();
    assert!(kernel.pending_aborts.is_empty());
}
