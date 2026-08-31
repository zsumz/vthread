use crate::{Runtime, ScopeOptions, ShutdownOutcome, control::Shared, signal::lock};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

#[test]
fn a_failed_carrier_join_cannot_report_successful_shutdown() {
    let runtime = Runtime::new().unwrap();
    *lock(&runtime.shared.carrier_exit_hook) = Some(Box::new(|| panic!("carrier exit failed")));
    let Err(crate::Error::ShutdownFailed(report)) = runtime.shutdown() else {
        panic!("carrier join panic was discarded");
    };
    assert_eq!(
        runtime.snapshot().shutdown_phase,
        crate::ShutdownPhase::Failed
    );
    let failure = &report.failures.entries()[0];
    assert_eq!(failure.component(), crate::ThreadComponent::Carrier);
    assert_eq!(failure.name(), "vthread-carrier-0");
    assert_eq!(failure.phase(), crate::FailurePhase::Join);
    assert_eq!(
        failure.shutdown_phase(),
        crate::ShutdownPhase::JoiningCarriers
    );
    assert_eq!(failure.panic().message(), "carrier exit failed");
    assert!(failure.cleanup_complete());
    runtime.request_shutdown();
    assert!(matches!(
        runtime.shutdown(),
        Err(crate::Error::ShutdownFailed(_))
    ));
}

#[test]
fn dropping_an_unrelated_runtime_does_not_block_a_carrier() {
    let outer = Runtime::new().unwrap();
    let inner = Runtime::new().unwrap();
    let (release, gate) = mpsc::sync_channel(1);
    // Retain the scope through Shared while deliberately transferring the last Runtime.
    let owner = inner
        .shared
        .begin_owned(ScopeOptions::default(), true)
        .unwrap();
    let mut job = inner
        .spawn(owner, "held carrier".into(), move || {
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        })
        .unwrap();
    let shared = Arc::clone(&inner.shared);
    crate::support_test::until(|| shared.snapshot().stats.mounts > 0);
    outer
        .run_scope(|scope| {
            let (done, receive) = mpsc::sync_channel(1);
            let mut dropper = scope.spawn("drop unrelated", move || {
                drop(inner);
                done.send(()).unwrap();
            })?;
            let progressed = receive.recv_timeout(Duration::from_millis(150)).is_ok();
            release.send(()).unwrap();
            dropper.join()?;
            let _ = job.join();
            assert!(progressed, "final-owner Drop blocked a foreign carrier");
            Ok(())
        })
        .unwrap();
}

#[test]
fn completion_waits_for_the_coordinators_thread_local_destructors() {
    struct Capture(mpsc::SyncSender<()>, mpsc::Receiver<()>);
    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.send(()).unwrap();
            self.1.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }
    thread_local! {
        static CAPTURE: std::cell::RefCell<Option<Capture>> = const { std::cell::RefCell::new(None) };
    }
    let runtime = Runtime::new().unwrap();
    let (started, entered) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    *lock(&runtime.shared.coordinator_exit_hook) = Some(Box::new(move || {
        CAPTURE.with(|slot| *slot.borrow_mut() = Some(Capture(started, gate)));
    }));
    let _ = runtime.shutdown_until(Instant::now()).unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let observed = runtime.shutdown_until(Instant::now() + Duration::from_millis(20));
    release.send(()).unwrap();
    assert!(
        matches!(observed, Ok(ShutdownOutcome::TimedOut(_))),
        "coordinator was not joined"
    );
    runtime.shutdown().unwrap();
}

#[test]
fn reciprocal_final_owner_drops_complete_without_join_cycles() {
    const CHILD: &str = "VTHREAD_RECIPROCAL_DROP_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "runtime::runtime_lifecycle::runtime_ownership_test::reciprocal_final_owner_drops_complete_without_join_cycles", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert!(
            output.status.success(),
            "isolated cycle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "cycle child did not execute its test"
        );
        return;
    }
    let (finished, watchdog) = mpsc::sync_channel(1);
    let watcher = thread::spawn(move || {
        if watchdog.recv_timeout(Duration::from_secs(3)).is_err() {
            std::process::exit(23);
        }
    });
    let left = Arc::new(Runtime::new().unwrap());
    let right = Arc::new(Runtime::new().unwrap());
    let a: Arc<Shared> = Arc::clone(&left.shared);
    let b = Arc::clone(&right.shared);
    let barrier = Arc::new(std::sync::Barrier::new(3));
    for (runtime, other) in [(&left, Arc::clone(&right)), (&right, Arc::clone(&left))] {
        let owner = runtime
            .shared
            .begin_owned(ScopeOptions::default(), true)
            .unwrap();
        let ready = Arc::clone(&barrier);
        let _ = runtime
            .spawn(owner, "reciprocal drop".into(), move || {
                ready.wait();
                drop(other);
            })
            .unwrap();
    }
    drop(left);
    drop(right);
    barrier.wait();
    crate::support_test::until(|| a.snapshot().active + b.snapshot().active == 0);
    finished.send(()).unwrap();
    watcher.join().unwrap();
}
#[test]
fn root_callbacks_remain_caller_owned_after_concurrent_shutdown() {
    use std::{
        sync::{Arc, mpsc},
        thread,
        time::Duration,
    };
    for generic in [false, true] {
        let runtime = Arc::new(crate::Runtime::new().unwrap());
        let caller = Arc::clone(&runtime);
        let (entered, ready) = mpsc::sync_channel(1);
        let (release, resume) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let body = |scope: &crate::Scope<'_>| {
                scope.spawn("owned child", || ())?.join()?;
                entered.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(
                    caller.snapshot().shutdown_phase(),
                    crate::ShutdownPhase::Complete
                );
                assert!(matches!(
                    scope.spawn("late", || ()),
                    Err(crate::Error::RuntimeStopped)
                ));
                Ok::<_, crate::Error>(42)
            };
            if generic {
                caller.try_run_scope(body).unwrap()
            } else {
                caller.run_scope(body).unwrap()
            }
        });
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        runtime.shutdown().unwrap();
        assert!(
            !worker.is_finished(),
            "shutdown waited for or stopped the caller callback"
        );
        release.send(()).unwrap();
        assert_eq!(worker.join().unwrap(), 42);
    }
}
