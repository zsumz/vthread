use crate::{
    CarrierId, RuntimeConfig, TaskFailure, control::Shared, kernel::Kernel,
    kernel_tasks::BorrowedTask, options::TaskOptions, task_context::TaskContext,
    task_fiber::BorrowedFiber,
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
        let data = Rc::new(TaskContext::new(record.lock().options().clone(), 1));
        let (id, scope) = {
            let record = record.lock();
            (record.id, record.scope)
        };
        let execution = Rc::new(crate::context::Execution::new(
            id,
            scope,
            Arc::clone(&kernel.inbox.hub),
            record,
            Arc::clone(&shared),
            Rc::clone(&kernel.local),
            Rc::clone(&data),
        ));
        #[cfg(feature = "runtime-evidence")]
        let fiber = BorrowedFiber::new(lease, identity);
        #[cfg(not(feature = "runtime-evidence"))]
        let fiber = BorrowedFiber::new(lease);
        kernel.local.push_start(BorrowedTask {
            fiber: Some(fiber),
            execution: Some(execution),
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
        let data = Rc::new(TaskContext::new(record.lock().options().clone(), 1));
        let (id, root) = {
            let record = record.lock();
            (record.id, record.scope)
        };
        let execution = Rc::new(crate::context::Execution::new(
            id,
            root,
            Arc::clone(&kernel.inbox.hub),
            Arc::clone(&record),
            Arc::clone(&shared),
            Rc::clone(&kernel.local),
            Rc::clone(&data),
        ));
        #[cfg(feature = "runtime-evidence")]
        let task_fiber = BorrowedFiber::new(lease, identity);
        #[cfg(not(feature = "runtime-evidence"))]
        let task_fiber = BorrowedFiber::new(lease);
        let task = kernel.tasks.insert_borrowed(BorrowedTask {
            execution: Some(execution),
            fiber: Some(task_fiber),
        });
        kernel.ready.push_back(task);
        kernel.has_borrowed = true;
        kernel.tick(true).unwrap();
        assert_eq!(kernel.parked.len(), 1);
    });
    kernel.tick(true).unwrap();
    let snapshot = shared.snapshot();
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.timers, 0);
    assert_eq!(snapshot.tasks[0].failure, Some(TaskFailure::ScopeClosed));
}

#[test]
fn revoked_synchronization_wait_releases_its_ticket() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let record = shared.reserve(scope, "revoked-sync".into(), None).unwrap();
    let mutex = crate::sync::Mutex::with_wait_capacity((), 1).unwrap();
    let owner = mutex.try_lock().unwrap();
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
                let _guard = mutex.lock().unwrap();
            })
            .unwrap();
        let data = Rc::new(TaskContext::new(record.lock().options().clone(), 1));
        let (id, root) = {
            let record = record.lock();
            (record.id, record.scope)
        };
        let execution = Rc::new(crate::context::Execution::new(
            id,
            root,
            Arc::clone(&kernel.inbox.hub),
            Arc::clone(&record),
            Arc::clone(&shared),
            Rc::clone(&kernel.local),
            Rc::clone(&data),
        ));
        #[cfg(feature = "runtime-evidence")]
        let task_fiber = BorrowedFiber::new(lease, identity);
        #[cfg(not(feature = "runtime-evidence"))]
        let task_fiber = BorrowedFiber::new(lease);
        let task = kernel.tasks.insert_borrowed(BorrowedTask {
            execution: Some(execution),
            fiber: Some(task_fiber),
        });
        kernel.ready.push_back(task);
        kernel.has_borrowed = true;

        kernel.tick(true).unwrap();
        assert_eq!(kernel.parked.len(), 1);
        assert_eq!(mutex.waiting(), 1);
    });

    kernel.tick(true).unwrap();
    assert_eq!(mutex.waiting(), 0);
    drop(owner);
    assert!(mutex.try_lock().is_ok());
    assert_eq!(shared.snapshot().active, 0);
}
