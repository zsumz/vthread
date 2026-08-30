use crate::{CarrierId, RuntimeConfig, TaskFailure, control::Shared, kernel::Kernel};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct DropCount(Arc<AtomicUsize>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn fault_cleanup_keeps_both_the_pending_packet_and_queued_packets_owned() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    let drops = Arc::new(AtomicUsize::new(0));
    for _ in 0..2 {
        let guard = DropCount(Arc::clone(&drops));
        shared
            .submit(scope, "unstarted".into(), move || drop(guard))
            .expect("submit");
    }
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.pending = kernel.inbox.pop();
    kernel.abort(None, TaskFailure::CarrierFailed);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    assert_eq!(shared.snapshot().active, 0);
    assert!(
        shared
            .snapshot()
            .tasks
            .iter()
            .all(|task| task.failure == Some(TaskFailure::CarrierFailed))
    );
}

#[test]
fn delayed_abort_for_an_old_scope_preserves_every_new_scope_queue() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let old = shared.begin_scope().expect("old scope");
    shared.finish_scope(old);
    let current = shared.begin_scope().expect("new scope");
    let (parker, _unparker) = crate::park_pair();
    shared
        .submit(current, "parked".into(), move || parker.park())
        .expect("parked");
    shared
        .submit(current, "ready".into(), || ())
        .expect("ready");
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    kernel.tick().expect("park the first task");
    shared
        .submit(current, "queued".into(), || ())
        .expect("queued");

    kernel.abort(Some(old), TaskFailure::ScopeStalled);
    let snapshot = shared.snapshot();
    assert_eq!(snapshot.active, 3);
    assert_eq!(snapshot.parked, 1);
    assert_eq!(snapshot.runnable, 2);
    assert!(snapshot.tasks.iter().all(|task| task.failure.is_none()));
    kernel.abort(None, TaskFailure::RuntimeStopped);
}
