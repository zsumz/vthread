use super::{Condvar, Mutex, Notify, Semaphore};
use crate::{Error, Runtime, ScopeOptions, support_test::until};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn blocking_sync_calls_require_virtual_context_even_when_ready() {
    let mutex = Mutex::new(42, 1).unwrap();
    assert!(matches!(mutex.lock(), Err(Error::OutsideVThread)));
    let semaphore = Semaphore::new(1, 1).unwrap();
    assert!(matches!(semaphore.acquire(), Err(Error::OutsideVThread)));
    let notify = Notify::new(1).unwrap();
    notify.notify_one();
    assert!(matches!(notify.notified(), Err(Error::OutsideVThread)));
    let changed = Condvar::new(1).unwrap();
    assert!(matches!(
        changed.wait(mutex.try_lock().unwrap()),
        Err(Error::OutsideVThread)
    ));
    drop(mutex.try_lock().unwrap());
}

#[test]
fn deadlines_remove_waiters_and_return_selected_resources() {
    let runtime = Runtime::new().unwrap();
    let semaphore = Arc::new(Semaphore::new(1, 1).unwrap());
    let permit = semaphore.try_acquire().unwrap();
    let options = ScopeOptions::default().deadline(Instant::now() + Duration::from_millis(20));
    runtime
        .scope_with(options, |scope| {
            let shared = Arc::clone(&semaphore);
            let waiter = scope.spawn("deadline", move || shared.acquire().map(drop))?;
            assert!(matches!(waiter.join()?, Err(Error::DeadlineExceeded)));
            Ok(())
        })
        .unwrap();
    assert_eq!(semaphore.waiting(), 0);
    drop(permit);
    assert_eq!(semaphore.available_permits(), 1);
}

#[test]
fn forced_shutdown_drops_held_guards_and_wait_tickets() {
    let runtime = Runtime::new().unwrap();
    let mutex = Arc::new(Mutex::new(0, 2).unwrap());
    let notify = Arc::new(Notify::new(1).unwrap());
    runtime
        .scope(|scope| {
            let shared = Arc::clone(&mutex);
            let event = Arc::clone(&notify);
            let owner = scope.spawn("owner", move || {
                let mut guard = shared.lock().unwrap();
                *guard = 42;
                event.notified()
            })?;
            until(|| notify.waiting() == 1);
            let shared = Arc::clone(&mutex);
            let waiter = scope.spawn("waiter", move || shared.lock().map(drop))?;
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
    let semaphore = Arc::new(Semaphore::new(1, 1).unwrap());
    let permit = semaphore.try_acquire().unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    runtime
        .scope(|scope| {
            let shared = Arc::clone(&semaphore);
            let waiter = scope.spawn("selected", move || {
                let _permit = shared.acquire().unwrap();
                panic!("selected waiter must be reclaimed before resumption");
            })?;
            until(|| semaphore.waiting() == 1);
            let entered = Arc::clone(&started);
            let released = Arc::clone(&release);
            let blocker = scope.spawn("test-interleaving", move || {
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
    use crate::{SuspensionReason, TaskStatus, channel::bounded};
    fn check(reason: SuspensionReason, body: impl FnOnce() -> crate::Result<()> + Send + 'static) {
        let runtime = Runtime::new().unwrap();
        runtime
            .scope(|scope| {
                let task = scope.spawn("diagnostic-wait", body)?;
                until(|| {
                    scope
                        .snapshot()
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
    let semaphore = Arc::new(Semaphore::new(1, 1).unwrap());
    let _permit = semaphore.try_acquire().unwrap();
    let shared = Arc::clone(&semaphore);
    check(SuspensionReason::Semaphore, move || {
        shared.acquire().map(drop)
    });
    let mutex = Arc::new(Mutex::new(0, 1).unwrap());
    let _guard = mutex.try_lock().unwrap();
    let shared = Arc::clone(&mutex);
    check(SuspensionReason::Mutex, move || shared.lock().map(drop));
    check(SuspensionReason::Notify, || Notify::new(1)?.notified());
    check(SuspensionReason::Condvar, || {
        let mutex = Mutex::new(0, 1)?;
        Condvar::new(1)?.wait(mutex.lock()?).map(drop)
    });
    let (sender, receiver) = bounded(1, 1).unwrap();
    sender.try_send(0).unwrap();
    check(SuspensionReason::ChannelSend, move || {
        sender.send(1).map_err(|error| error.error)
    });
    drop(receiver);
    let (_sender, receiver) = bounded::<u8>(1, 1).unwrap();
    check(SuspensionReason::ChannelRecv, move || {
        receiver.recv().map(drop)
    });
}
