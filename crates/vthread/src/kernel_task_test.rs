use crate::{CarrierId, RuntimeConfig, TaskFailure, control::Shared, kernel::Kernel};
use std::sync::Arc;
use vthread_stack::{FiberState, Suspension};

#[test]
fn checkpointing_dispatch_keeps_its_task_slot_and_progress() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().expect("scope");
    shared
        .submit(scope, "yielding".into(), || crate::yield_now().unwrap())
        .expect("task");
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    let key = *kernel.ready.front().expect("ready task");
    let address = kernel.tasks.get(key).expect("task slot").address();
    let carrier = &kernel.shared.carrier_progress[kernel.id.0];

    let mut task = kernel.tasks.get_mut(key).expect("task slot");
    let execution = task.execution();
    assert!(execution.progress.mount(carrier, execution.id));
    assert!(matches!(
        task.dispatch(&shared, carrier),
        Some(FiberState::Suspended(Suspension::YieldNow))
    ));

    assert_eq!(kernel.tasks.get(key).expect("task slot").address(), address);
    let snapshot = kernel
        .tasks
        .get(key)
        .expect("task slot")
        .execution()
        .record()
        .snapshot(&[carrier.mounted()]);
    assert_eq!(
        snapshot.last_suspension,
        Some(crate::SuspensionReason::YieldNow)
    );
    kernel.abort(None, TaskFailure::RuntimeStopped);
}
