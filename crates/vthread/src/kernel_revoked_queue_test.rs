use crate::{
    CarrierId, RuntimeConfig, TaskFailure, context::Execution, control::Shared, kernel::Kernel,
    kernel_tasks::BorrowedTask, task_context::TaskContext, task_fiber::BorrowedFiber,
};
use std::{rc::Rc, sync::Arc};

#[test]
fn revoked_middle_wake_is_reclaimed_before_the_epoch_is_consumed() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    for index in 0..3 {
        shared
            .submit(scope, format!("owned-{index}"), || ())
            .unwrap();
    }
    let record = shared.reserve(scope, "revoked".into(), None).unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    let owned = (0..3)
        .map(|_| kernel.ready.pop_front().unwrap())
        .collect::<Vec<_>>();
    let revoked = vthread_stack::fiber_scope(1, |fibers| {
        #[cfg(feature = "runtime-evidence")]
        let (identity, stack) = kernel
            .local
            .stacks
            .borrow_mut()
            .acquire_identified()
            .unwrap();
        #[cfg(not(feature = "runtime-evidence"))]
        let stack = kernel.local.stacks.borrow_mut().acquire().unwrap();
        let lease = fibers.spawn(stack, || ()).unwrap();
        let data = Rc::new(TaskContext::new(record.lock().options().clone(), 1));
        let id = record.lock().id;
        let execution = Rc::new(Execution::new(
            id,
            scope,
            Arc::clone(&kernel.inbox.hub),
            record,
            Arc::clone(&shared),
            Rc::clone(&kernel.local),
            data,
        ));
        #[cfg(feature = "runtime-evidence")]
        let fiber = BorrowedFiber::new(lease, identity);
        #[cfg(not(feature = "runtime-evidence"))]
        let fiber = BorrowedFiber::new(lease);
        kernel.tasks.insert_borrowed(BorrowedTask {
            execution: Some(execution),
            fiber: Some(fiber),
        })
    });
    for task in [owned[0], revoked, owned[1], owned[2]] {
        kernel.ready.push_wake(task);
    }
    kernel.has_borrowed = true;
    kernel.local.publish_borrowed_scope_exit();

    kernel.sweep_revoked();

    assert_eq!(kernel.tasks.borrowed_count(), 0);
    assert_eq!(kernel.ready.len(), 3);
    assert_eq!(kernel.revocation_inspections, 4);
    assert!(!kernel.has_borrowed);
    kernel.abort(None, TaskFailure::RuntimeStopped);
}
