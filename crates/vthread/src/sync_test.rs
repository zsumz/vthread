use super::{Condvar, Mutex, Notify, Semaphore};
use crate::{Error, Runtime, ScopeOptions, support_test::until};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn simple_constructors_expose_bounded_wait_capacity() {
    let mutex = Mutex::new(42);
    assert_eq!(mutex.wait_capacity(), super::DEFAULT_WAIT_CAPACITY);
    assert_eq!(*mutex.try_lock().unwrap(), 42);
    let semaphore = Semaphore::new(2).unwrap();
    assert_eq!(semaphore.wait_capacity(), super::DEFAULT_WAIT_CAPACITY);
    let permit = semaphore.try_acquire().unwrap();
    assert_eq!(semaphore.available_permits(), 1);
    drop(permit);
    assert_eq!(semaphore.available_permits(), 2);
    assert!(Semaphore::new(0).is_err());
    let notify = Notify::new();
    assert_eq!(notify.wait_capacity(), super::DEFAULT_WAIT_CAPACITY);
    notify.notify_one();
    notify.try_notified().unwrap();
    let changed = Condvar::new();
    assert_eq!(changed.wait_capacity(), super::DEFAULT_WAIT_CAPACITY);
    changed.notify_all();
    assert!(Mutex::with_wait_capacity(0, 0).is_err());
    assert!(Semaphore::with_wait_capacity(1, 0).is_err());
    assert!(Notify::with_wait_capacity(0).is_err());
    assert!(Condvar::with_wait_capacity(0).is_err());
    let default_mutex = Mutex::<usize>::default();
    assert_eq!(*default_mutex.try_lock().unwrap(), 0);
    assert_eq!(
        Notify::default().wait_capacity(),
        super::DEFAULT_WAIT_CAPACITY
    );
    assert_eq!(
        Condvar::default().wait_capacity(),
        super::DEFAULT_WAIT_CAPACITY
    );
}

#[test]
fn default_waiter_budget_rejects_overflow_without_losing_existing_waits() {
    let runtime = Runtime::new().unwrap();
    let notify = Arc::new(Notify::new());
    runtime
        .run_scope(|scope| {
            let mut waits = Vec::new();
            for _ in 0..super::DEFAULT_WAIT_CAPACITY {
                let notify = Arc::clone(&notify);
                waits.push(scope.spawn("default-waiter", move || notify.notified())?);
            }
            until(|| notify.waiting() == super::DEFAULT_WAIT_CAPACITY);
            let overflow = Arc::clone(&notify);
            let error = scope
                .spawn("overflow", move || overflow.notified())?
                .join()?
                .unwrap_err();
            assert!(matches!(
                error,
                Error::Capacity {
                    resource: crate::error::CapacityResource::Waiters,
                    limit: super::DEFAULT_WAIT_CAPACITY,
                }
            ));
            notify.notify_waiters();
            for mut wait in waits {
                wait.join()??;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(notify.waiting(), 0);
}

#[test]
fn blocking_sync_calls_require_virtual_context_even_when_ready() {
    let mutex = Mutex::with_wait_capacity(42, 1).unwrap();
    assert!(matches!(mutex.lock(), Err(Error::OutsideVThread)));
    let semaphore = Semaphore::with_wait_capacity(1, 1).unwrap();
    assert!(matches!(semaphore.acquire(), Err(Error::OutsideVThread)));
    let notify = Notify::with_wait_capacity(1).unwrap();
    notify.notify_one();
    assert!(matches!(notify.notified(), Err(Error::OutsideVThread)));
    let changed = Condvar::with_wait_capacity(1).unwrap();
    assert!(matches!(
        changed.wait(mutex.try_lock().unwrap()),
        Err(Error::OutsideVThread)
    ));
    drop(mutex.try_lock().unwrap());
}

#[test]
fn deadlines_remove_waiters_and_return_selected_resources() {
    let runtime = Runtime::new().unwrap();
    let semaphore = Arc::new(Semaphore::with_wait_capacity(1, 1).unwrap());
    let permit = semaphore.try_acquire().unwrap();
    let options = ScopeOptions::default().deadline(Instant::now() + Duration::from_millis(20));
    let error = runtime
        .run_scope_with(options, |scope| {
            let shared = Arc::clone(&semaphore);
            let mut waiter = scope.spawn("deadline", move || shared.acquire().map(drop))?;
            assert!(matches!(waiter.join()?, Err(Error::DeadlineExceeded)));
            Ok(())
        })
        .unwrap_err();
    assert!(matches!(error.primary(), Error::DeadlineExceeded));
    assert_eq!(semaphore.waiting(), 0);
    drop(permit);
    assert_eq!(semaphore.available_permits(), 1);
}

#[test]
fn forced_shutdown_drops_held_guards_and_wait_tickets() {
    let runtime = Runtime::new().unwrap();
    let mutex = Arc::new(Mutex::with_wait_capacity(0, 2).unwrap());
    let notify = Arc::new(Notify::with_wait_capacity(1).unwrap());
    runtime
        .run_scope(|scope| {
            let shared = Arc::clone(&mutex);
            let event = Arc::clone(&notify);
            let mut owner = scope.spawn("owner", move || {
                let mut guard = shared.lock().unwrap();
                *guard = 42;
                event.notified()
            })?;
            until(|| notify.waiting() == 1);
            let shared = Arc::clone(&mutex);
            let mut waiter = scope.spawn("waiter", move || shared.lock().map(drop))?;
            until(|| mutex.waiting() == 1);
            runtime.shutdown()?;
            assert!(matches!(owner.join(), Err(Error::TaskAborted { .. })));
            assert!(matches!(waiter.join(), Err(Error::TaskAborted { .. })));
            Ok(())
        })
        .unwrap();
    assert_eq!(notify.waiting(), 0);
    assert_eq!(mutex.waiting(), 0);
    assert_eq!(*mutex.try_lock().unwrap(), 42);
}

#[test]
fn shutdown_after_selection_returns_the_reserved_permit_without_resuming_waiter() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let runtime = Runtime::new().unwrap();
    let semaphore = Arc::new(Semaphore::with_wait_capacity(1, 1).unwrap());
    let permit = semaphore.try_acquire().unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    runtime
        .run_scope(|scope| {
            let shared = Arc::clone(&semaphore);
            let mut waiter = scope.spawn("selected", move || {
                let _permit = shared.acquire().unwrap();
                panic!("selected waiter must be reclaimed before resumption");
            })?;
            until(|| semaphore.waiting() == 1);
            let entered = Arc::clone(&started);
            let released = Arc::clone(&release);
            let mut blocker = scope.spawn("test-interleaving", move || {
                entered.store(true, Ordering::SeqCst);
                // Deliberately pin this test carrier to control the select/stop ordering.
                until(|| released.load(Ordering::SeqCst));
            })?;
            until(|| started.load(Ordering::SeqCst));
            drop(permit);
            runtime.shared.request_stop();
            release.store(true, Ordering::SeqCst);
            runtime.shutdown()?;
            assert!(matches!(waiter.join(), Err(Error::TaskAborted { .. })));
            blocker.join()?;
            Ok(())
        })
        .unwrap();
    assert_eq!(semaphore.waiting(), 0);
    assert_eq!(semaphore.available_permits(), 1);
}

#[test]
fn task_dumps_identify_every_synchronization_boundary() {
    use crate::{SuspensionReason, TaskStatus, channel::bounded_with_wait_capacity};
    fn check(reason: SuspensionReason, body: impl FnOnce() -> crate::Result<()> + Send + 'static) {
        let runtime = Runtime::new().unwrap();
        runtime
            .run_scope(|scope| {
                let mut task = scope.spawn("diagnostic-wait", body)?;
                until(|| {
                    scope
                        .runtime_snapshot()
                        .tasks
                        .iter()
                        .any(|task| task.status == TaskStatus::Suspended(reason))
                });
                scope.cancel();
                assert!(matches!(task.join()?, Err(Error::Cancelled)));
                Ok(())
            })
            .unwrap();
    }
    let semaphore = Arc::new(Semaphore::with_wait_capacity(1, 1).unwrap());
    let _permit = semaphore.try_acquire().unwrap();
    let shared = Arc::clone(&semaphore);
    check(SuspensionReason::Semaphore, move || {
        shared.acquire().map(drop)
    });
    let mutex = Arc::new(Mutex::with_wait_capacity(0, 1).unwrap());
    let _guard = mutex.try_lock().unwrap();
    let shared = Arc::clone(&mutex);
    check(SuspensionReason::Mutex, move || shared.lock().map(drop));
    check(SuspensionReason::Notify, || {
        Notify::with_wait_capacity(1)?.notified()
    });
    check(SuspensionReason::Condvar, || {
        let mutex = Mutex::with_wait_capacity(0, 1)?;
        Condvar::with_wait_capacity(1)?
            .wait(mutex.lock()?)
            .map(drop)
    });
    let (sender, receiver) = bounded_with_wait_capacity(1, 1).unwrap();
    sender.try_send(0).unwrap();
    check(SuspensionReason::ChannelSend, move || {
        sender.send(1).map_err(|error| error.error)
    });
    drop(receiver);
    let (_sender, receiver) = bounded_with_wait_capacity::<u8>(1, 1).unwrap();
    check(SuspensionReason::ChannelRecv, move || {
        receiver.recv().map(drop)
    });
}
