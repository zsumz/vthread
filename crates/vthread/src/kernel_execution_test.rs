use super::Kernel;
use crate::{CarrierId, Runtime, control::Shared};
use std::{rc::Rc, sync::Arc};

#[test]
fn completed_execution_storage_is_reset_and_reused() {
    let config = Runtime::builder()
        .max_vthreads(1)
        .carrier_queue_capacity(1)
        .stack_cache_capacity(1)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));

    let first_scope = shared.begin_scope().unwrap();
    shared.submit(first_scope, "first".into(), || ()).unwrap();
    kernel.receive();
    let first = *kernel.ready.front().unwrap();
    let address = Rc::as_ptr(kernel.task(first).execution());
    assert!(kernel.tick(true).unwrap());
    assert_eq!(kernel.execution_cache.len(), 1);
    shared.finish_scope(first_scope);

    let second_scope = shared.begin_scope().unwrap();
    shared
        .submit(second_scope, "second".into(), crate::checkpoint)
        .unwrap();
    kernel.receive();
    let second = *kernel.ready.front().unwrap();
    assert_eq!(Rc::as_ptr(kernel.task(second).execution()), address);
    assert!(kernel.tick(true).unwrap());
    shared.finish_scope(second_scope);
}

#[test]
fn closing_a_semaphore_does_not_poison_the_tasks_next_mutex_wait() {
    let shared = Arc::new(Shared::new(crate::RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let semaphore = Arc::new(crate::sync::Semaphore::with_wait_capacity(1, 1).unwrap());
    let _permit = semaphore.try_acquire().unwrap();
    let mutex = Arc::new(crate::sync::Mutex::with_wait_capacity(42, 1).unwrap());
    let guard = mutex.try_lock().unwrap();
    let task_semaphore = Arc::clone(&semaphore);
    let task_mutex = Arc::clone(&mutex);
    shared
        .submit(scope, "close-then-lock".into(), move || {
            assert!(matches!(
                task_semaphore.acquire(),
                Err(crate::Error::Closed)
            ));
            assert_eq!(*task_mutex.lock().expect("unrelated mutex wait"), 42);
        })
        .unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    assert!(kernel.tick(true).unwrap());
    assert_eq!(semaphore.waiting(), 1);
    semaphore.close();
    assert!(kernel.tick(true).unwrap());
    assert_eq!(semaphore.waiting(), 0);
    assert_eq!(mutex.waiting(), 1, "the same task must reach its next wait");
    drop(guard);
    assert!(kernel.tick(true).unwrap());
    assert!(!kernel.tick(false).unwrap());
    shared.wait(scope, None).unwrap();
    shared.finish_scope(scope);
}

#[test]
fn closed_synchronization_waits_do_not_poison_recycled_execution() {
    let config = Runtime::builder()
        .max_vthreads(1)
        .carrier_queue_capacity(1)
        .stack_cache_capacity(1)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    let mut previous_execution = None;
    for _ in 0..3 {
        let scope = shared.begin_scope().unwrap();
        let semaphore = Arc::new(crate::sync::Semaphore::with_wait_capacity(1, 1).unwrap());
        let _permit = semaphore.try_acquire().unwrap();
        let task_semaphore = Arc::clone(&semaphore);
        shared
            .submit(scope, "closed-gate".into(), move || {
                assert!(matches!(
                    task_semaphore.acquire(),
                    Err(crate::Error::Closed)
                ));
            })
            .unwrap();
        kernel.receive();
        let address = Rc::as_ptr(kernel.task(*kernel.ready.front().unwrap()).execution());
        if let Some(previous) = previous_execution {
            assert_eq!(address, previous, "reuse the same execution storage");
        }
        previous_execution = Some(address);
        assert!(kernel.tick(true).unwrap());
        assert_eq!(semaphore.waiting(), 1, "reused storage must still park");
        semaphore.close();
        assert!(kernel.tick(true).unwrap());
        assert!(!kernel.tick(false).unwrap());
        shared.wait(scope, None).unwrap();
        shared.finish_scope(scope);
        assert_eq!(kernel.execution_cache.len(), 1);
        assert!(semaphore.is_closed());
    }
}

#[test]
fn closing_a_condvar_does_not_poison_the_next_predicate_lock() {
    let shared = Arc::new(Shared::new(crate::RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let changed = Arc::new(crate::sync::Condvar::with_wait_capacity(1).unwrap());
    let mutex = Arc::new(crate::sync::Mutex::with_wait_capacity(42, 1).unwrap());
    let task_changed = Arc::clone(&changed);
    let task_mutex = Arc::clone(&mutex);
    shared
        .submit(scope, "close-then-relock".into(), move || {
            let guard = task_mutex.lock().unwrap();
            assert!(matches!(
                task_changed.wait(guard),
                Err(crate::Error::Closed)
            ));
            assert_eq!(*task_mutex.lock().expect("next predicate wait"), 42);
        })
        .unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    assert!(kernel.tick(true).unwrap());
    assert_eq!(changed.waiting(), 1);
    let guard = mutex
        .try_lock()
        .expect("condition wait released predicate lock");
    changed.close();
    assert!(kernel.tick(true).unwrap());
    assert_eq!(changed.waiting(), 0);
    assert_eq!(mutex.waiting(), 1);
    drop(guard);
    assert!(kernel.tick(true).unwrap());
    assert!(!kernel.tick(false).unwrap());
    shared.wait(scope, None).unwrap();
    shared.finish_scope(scope);
}
