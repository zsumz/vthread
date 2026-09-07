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

#[test]
fn deferred_abort_finds_an_unblocked_middle_wake() {
    let shared = Arc::new(Shared::new(crate::RuntimeConfig::default()));
    let blocked = shared.begin_scope().unwrap();
    let unblocked = shared
        .begin_owned(crate::ScopeOptions::default(), true)
        .unwrap();
    for (index, scope) in [blocked, unblocked, blocked, blocked]
        .into_iter()
        .enumerate()
    {
        shared
            .submit(scope, format!("wake-{index}"), || ())
            .unwrap();
    }
    let mut kernel = Kernel::new(shared, CarrierId(0));
    kernel.receive();
    let tasks = (0..4)
        .map(|_| kernel.ready.pop_front().unwrap())
        .collect::<Vec<_>>();
    for &task in &tasks {
        kernel.ready.push_wake(task);
    }
    kernel.defer_abort(Some(blocked), TaskFailure::ScopeStalled);

    let selected = kernel.select_unblocked();

    // Newest-to-oldest is blocked, blocked, eligible, blocked. A four-pop
    // scheduling rotation revisits the newest task and misses the eligible one.
    assert_eq!(selected, Some(tasks[1]));
    kernel.ready.push_back(selected.unwrap());
    kernel.abort(None, TaskFailure::RuntimeStopped);
}
