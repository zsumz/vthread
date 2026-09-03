use crate::{
    CarrierId, RuntimeConfig, TaskFailure,
    control::Shared,
    kernel::{Kernel, Task},
    options::TaskOptions,
    task_context::TaskContext,
    task_fiber::TaskFiber,
};
use std::{rc::Rc, sync::Arc, time::Duration};

#[test]
fn owned_ready_queues_skip_borrowed_lease_scans() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    for index in 0..64 {
        shared
            .submit(scope, format!("owned-{index}"), || ())
            .unwrap();
    }
    let mut kernel = Kernel::new(shared, CarrierId(0));
    kernel.receive();

    kernel.sweep_revoked();

    assert_eq!(kernel.revocation_inspections, 0);
    assert_eq!(kernel.ready.len(), 64);
}

#[test]
fn selective_abort_retains_borrowed_scan_tracking() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let aborted = shared.begin_scope().unwrap();
    let retained = shared
        .begin_owned(crate::ScopeOptions::default(), true)
        .unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    vthread_stack::fiber_scope(1, |fibers| {
        #[cfg(feature = "runtime-evidence")]
        let (identity, stack) = kernel
            .local
            .stacks
            .borrow_mut()
            .acquire_identified()
            .unwrap();
        #[cfg(not(feature = "runtime-evidence"))]
        let stack = kernel.local.stacks.borrow_mut().acquire().unwrap();
        let lease = fibers.spawn(stack, || {}).unwrap();
        let record = shared
            .reserve(
                retained,
                "retained".into(),
                Some((
                    CarrierId(0),
                    crate::TaskId::new(99),
                    TaskOptions::root(crate::ScopeOptions::default(), 1),
                )),
            )
            .unwrap();
        let data = Rc::new(TaskContext::new(record.lock().options.clone(), 1));
        let (id, scope) = {
            let record = record.lock();
            (record.id, record.scope)
        };
        let execution = Rc::new(crate::context::Execution {
            id,
            scope,
            hub: Arc::clone(&kernel.inbox.hub),
            record,
            shared: Arc::clone(&shared),
            local: Rc::clone(&kernel.local),
            data,
            progress: crate::task_progress::TaskProgressWriter::new(),
        });
        #[cfg(feature = "runtime-evidence")]
        let fiber = TaskFiber::borrowed(lease, identity);
        #[cfg(not(feature = "runtime-evidence"))]
        let fiber = TaskFiber::borrowed(lease);
        kernel.local.push_start(Task {
            fiber: Some(fiber),
            execution,
            checkpoint_on_resume: false,
        });

        kernel.abort(Some(aborted), crate::TaskFailure::ScopeStalled);

        assert!(kernel.has_borrowed);
        kernel.abort(None, crate::TaskFailure::RuntimeStopped);
    });
}

#[test]
fn revoked_parked_stacks_release_registrations_before_timer_processing() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let record = shared.reserve(scope, "revoked".into(), None).unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    vthread_stack::fiber_scope(1, |fibers| {
        #[cfg(feature = "runtime-evidence")]
        let (identity, stack) = kernel
            .local
            .stacks
            .borrow_mut()
            .acquire_identified()
            .unwrap();
        #[cfg(not(feature = "runtime-evidence"))]
        let stack = kernel.local.stacks.borrow_mut().acquire().unwrap();
        let lease = fibers
            .spawn(stack, || {
                let (parker, _waker) = crate::park_pair();
                parker.park_timeout(Duration::from_secs(5)).unwrap();
            })
            .unwrap();
        let data = Rc::new(TaskContext::new(record.lock().options.clone(), 1));
        let (id, root) = {
            let record = record.lock();
            (record.id, record.scope)
        };
        let execution = Rc::new(crate::context::Execution {
            id,
            scope: root,
            hub: Arc::clone(&kernel.inbox.hub),
            record: Arc::clone(&record),
            shared: Arc::clone(&shared),
            local: Rc::clone(&kernel.local),
            data,
            progress: crate::task_progress::TaskProgressWriter::new(),
        });
        #[cfg(feature = "runtime-evidence")]
        let task_fiber = TaskFiber::borrowed(lease, identity);
        #[cfg(not(feature = "runtime-evidence"))]
        let task_fiber = TaskFiber::borrowed(lease);
        let task = kernel.tasks.insert(Task {
            execution,
            fiber: Some(task_fiber),
            checkpoint_on_resume: false,
        });
        kernel.ready.push_back(task);
        kernel.has_borrowed = true;
        kernel.tick(true).unwrap();
        assert_eq!(shared.snapshot().parked, 1);
    });
    kernel.tick(true).unwrap();
    let snapshot = shared.snapshot();
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.timers, 0);
    assert_eq!(snapshot.tasks[0].failure, Some(TaskFailure::ScopeClosed));
}
