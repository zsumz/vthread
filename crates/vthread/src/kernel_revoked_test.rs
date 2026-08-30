use crate::{
    CarrierId, RuntimeConfig, TaskFailure,
    control::Shared,
    kernel::{Kernel, Task},
    signal::lock,
    task_context::TaskContext,
    task_fiber::TaskFiber,
};
use std::{rc::Rc, sync::Arc, time::Duration};

#[test]
fn revoked_parked_stacks_release_registrations_before_timer_processing() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let record = shared.reserve(scope, "revoked".into(), None).unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    vthread_stack::fiber_scope(1, |fibers| {
        let lease = fibers
            .spawn(kernel.local.stacks.borrow_mut().acquire().unwrap(), || {
                let (parker, _waker) = crate::park_pair();
                parker.park_timeout(Duration::from_secs(5)).unwrap();
            })
            .unwrap();
        let data = Rc::new(TaskContext::new(lock(&record).options.clone(), 1));
        kernel.ready.push_back(Task {
            record: Arc::clone(&record),
            data,
            fiber: Some(TaskFiber::Borrowed(lease)),
        });
        kernel.tick().unwrap();
        assert_eq!(shared.snapshot().parked, 1);
    });
    kernel.tick().unwrap();
    let snapshot = shared.snapshot();
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.timers, 0);
    assert_eq!(snapshot.tasks[0].failure, Some(TaskFailure::ScopeClosed));
}
