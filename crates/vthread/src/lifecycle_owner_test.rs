use super::{LIFECYCLE_CAPACITY as CAPACITY, Permit, State};
use crate::{Error, signal::lock};
use std::sync::Arc;

#[test]
fn reservation_is_bounded_and_failed_start_releases_its_slot() {
    let state = Arc::new(State::default());
    let mut permits = Vec::new();
    for _ in 0..CAPACITY {
        permits.push(Permit::reserve(Arc::clone(&state)).unwrap());
    }
    assert!(matches!(
        Permit::reserve(Arc::clone(&state)),
        Err(Error::Capacity {
            resource: crate::error::CapacityResource::Lifecycles,
            ..
        })
    ));
    drop(permits.pop());
    let replacement = Permit::reserve(Arc::clone(&state)).unwrap();
    assert_eq!(lock(&state.slots).occupied, CAPACITY);
    drop(permits);
    drop(replacement);
    assert_eq!(lock(&state.slots).occupied, 0);
}

#[test]
fn coordinator_spawn_failure_returns_before_any_runtime_work_is_constructed() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let shared = Arc::new(crate::control::Shared::new(crate::RuntimeConfig::default()));
    shared.fail_coordinator_start.store(true, Ordering::Relaxed);
    let observed = Arc::downgrade(&shared);
    let ran = Arc::new(AtomicBool::new(false));
    let body_ran = Arc::clone(&ran);
    let result = super::start(Arc::clone(&shared), Arc::default(), move || {
        body_ran.store(true, Ordering::Relaxed);
    });
    assert!(matches!(
        result,
        Err(Error::ThreadStart {
            component: crate::ThreadComponent::Coordinator,
            ..
        })
    ));
    assert!(!ran.load(Ordering::Relaxed));
    assert!(shared.services.get().is_none());
    assert_eq!(shared.snapshot().active(), 0);
    drop(shared);
    assert!(
        observed.upgrade().is_none(),
        "failed start retained a runtime owner"
    );
}

#[test]
fn admission_detects_a_finished_owner_without_a_published_failure() {
    let owner = super::Owner {
        state: Arc::new(State::default()),
        worker: std::thread::spawn(|| {}),
    };
    crate::support_test::until(|| owner.worker.is_finished());
    assert!(matches!(owner.health(), super::LifecycleHealth::Failed(_)));
    assert!(matches!(
        Permit::reserve(Arc::clone(&owner.state)),
        Err(Error::LifecycleFailed(_))
    ));
    owner.worker.join().unwrap();
}

#[test]
fn a_failed_process_owner_rejects_admission_and_wakes_shutdown_waiters() {
    const CHILD: &str = "VTHREAD_OWNER_FAILURE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "lifecycle_owner::lifecycle_owner_test::a_failed_process_owner_rejects_admission_and_wakes_shutdown_waiters", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert!(
            output.status.success(),
            "isolated owner failure: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    use std::{
        sync::{atomic::Ordering, mpsc},
        thread,
        time::{Duration, Instant},
    };
    let (finished, watchdog) = mpsc::sync_channel(1);
    let watcher = thread::spawn(move || {
        if watchdog.recv_timeout(Duration::from_secs(3)).is_err() {
            std::process::exit(24);
        }
    });
    assert_eq!(
        super::lifecycle_health(),
        super::LifecycleHealth::NotStarted
    );
    let runtime = Arc::new(crate::Runtime::new().unwrap());
    let (release, gate) = mpsc::sync_channel(1);
    *lock(&runtime.shared.coordinator_exit_hook) = Some(Box::new(move || {
        gate.recv_timeout(Duration::from_secs(2)).unwrap();
    }));
    let waiting = Arc::clone(&runtime);
    let waiter = thread::spawn(move || waiting.shutdown());
    crate::support_test::until(|| {
        runtime.snapshot().shutdown_phase() != crate::ShutdownPhase::NotRequested
    });
    {
        let owner = lock(super::OWNER.get().unwrap());
        let state = &owner.as_ref().unwrap().state;
        state.fail_at.store(1, Ordering::Release);
        state.changed.notify();
    }
    crate::support_test::until(|| {
        matches!(super::lifecycle_health(), super::LifecycleHealth::Failed(_))
    });
    let before = Instant::now();
    assert!(matches!(
        crate::Runtime::new(),
        Err(Error::LifecycleFailed(_))
    ));
    assert!(matches!(
        waiter.join().unwrap(),
        Err(Error::LifecycleFailed(_))
    ));
    assert!(matches!(
        runtime.shutdown_until(Instant::now()),
        Err(Error::LifecycleFailed(_))
    ));
    assert!(before.elapsed() < Duration::from_secs(1));
    assert!(
        runtime
            .snapshot()
            .failures()
            .entries()
            .iter()
            .any(|entry| entry.component() == crate::ThreadComponent::LifecycleOwner)
    );
    release.send(()).unwrap();
    drop(runtime);
    finished.send(()).unwrap();
    watcher.join().unwrap();
}

#[test]
fn opaque_worker_panic_cannot_run_a_blocking_drop_on_lifecycle_threads() {
    const CHILD: &str = "VTHREAD_OWNER_OPAQUE_PANIC_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "lifecycle_owner::lifecycle_owner_test::opaque_worker_panic_cannot_run_a_blocking_drop_on_lifecycle_threads", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert!(
            output.status.success(),
            "isolated opaque failure: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    use std::{
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    struct Payload(mpsc::SyncSender<()>, mpsc::Receiver<()>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.send(()).unwrap();
            self.1.recv_timeout(Duration::from_secs(30)).unwrap();
        }
    }
    let (finished, watchdog) = mpsc::sync_channel(1);
    let watcher = thread::spawn(move || {
        if watchdog.recv_timeout(Duration::from_secs(3)).is_err() {
            std::process::exit(25);
        }
    });
    let runtime = crate::Runtime::new().unwrap();
    let (dropped, observed) = mpsc::sync_channel(1);
    let (_release, gate) = mpsc::sync_channel(1);
    let payload = Payload(dropped, gate);
    *lock(&runtime.shared.carrier_exit_hook) = Some(Box::new(move || {
        std::panic::panic_any(payload);
    }));
    let Err(Error::ShutdownFailed(report)) = runtime.shutdown() else {
        panic!("opaque carrier failure was lost");
    };
    assert!(!report.failures().entries()[0].cleanup_complete());
    assert!(
        observed.try_recv().is_err(),
        "lifecycle thread executed opaque Drop"
    );
    let next = crate::Runtime::new().unwrap();
    assert!(matches!(
        next.shutdown_until(Instant::now() + Duration::from_secs(1)),
        Ok(crate::ShutdownOutcome::Complete(_))
    ));
    assert_eq!(super::lifecycle_health(), super::LifecycleHealth::Running);
    finished.send(()).unwrap();
    watcher.join().unwrap();
}
