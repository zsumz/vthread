use crate::{Error, Runtime, ShutdownOutcome, blocking, support_test::until};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

#[test]
fn stop_before_service_publication_still_drains_late_services() {
    let config = crate::RuntimeConfig::default();
    let shared = Arc::new(crate::control::Shared::new(config));
    let driver = super::ShutdownDriver::new(&shared).unwrap();
    shared.request_stop();
    assert!(
        shared
            .services
            .set(crate::services::Services::new(config, Arc::downgrade(&shared),).unwrap())
            .is_ok()
    );
    driver.ready(&shared);
    let deadline = Instant::now() + Duration::from_millis(200);
    let complete = loop {
        let observed = shared.changed.version();
        if shared.shutdown_phase() == crate::ShutdownPhase::Complete {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        shared.changed.wait(observed, Some(deadline));
    };
    // Even the old behavior is unblocked before the assertion so its failed case
    // cannot leave a live service behind in the rest of this test process.
    shared.services.get().unwrap().stop();
    assert!(
        complete,
        "late services missed stop and stranded the coordinator"
    );
}

#[test]
fn deadline_reports_running_native_work_and_retry_joins_it() {
    let runtime = Runtime::new().unwrap();
    let (release, receive) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            let mut task = scope.spawn("native", move || {
                blocking::run(move || receive.recv_timeout(Duration::from_secs(5)).unwrap())
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            let before = Instant::now();
            let outcome = runtime.shutdown_until(before + Duration::from_millis(20))?;
            let elapsed = before.elapsed();
            release.send(()).unwrap();
            let ShutdownOutcome::TimedOut(snapshot) = outcome else {
                panic!("unfinished native work was reported complete");
            };
            assert!(elapsed < Duration::from_secs(1));
            assert!(!snapshot.accepting);
            assert_eq!(snapshot.services.blocking_running, 1);
            assert!(matches!(
                scope.spawn("late", || ()),
                Err(Error::RuntimeStopped)
            ));
            let ShutdownOutcome::Complete(report) =
                runtime.shutdown_until(Instant::now() + Duration::from_secs(5))?
            else {
                panic!("released native work did not drain");
            };
            assert_eq!(runtime.shutdown()?, report);
            let ShutdownOutcome::Complete(repeated) = runtime.shutdown_until(Instant::now())?
            else {
                panic!("completed shutdown regressed");
            };
            assert_eq!(repeated, report);
            assert!(matches!(task.join(), Err(Error::TaskAborted { .. })));
            Ok(())
        })
        .unwrap();
}

#[test]
fn a_timed_wait_does_not_queue_behind_a_blocking_shutdown_join() {
    let runtime = Arc::new(Runtime::new().unwrap());
    let (release, receive) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            // Deliberately block a carrier to exercise the join lock, not the native pool.
            let mut task = scope.spawn("uncooperative", move || {
                receive.recv_timeout(Duration::from_secs(5)).unwrap();
            })?;
            until(|| runtime.snapshot().active == 1 && runtime.snapshot().stats.mounts > 0);
            let remote = Arc::clone(&runtime);
            let stopper = thread::spawn(move || remote.shutdown());
            until(|| runtime.snapshot().shutdown_phase == crate::ShutdownPhase::JoiningCarriers);
            let before = Instant::now();
            let outcome = runtime.shutdown_until(before + Duration::from_millis(20))?;
            let elapsed = before.elapsed();
            release.send(()).unwrap();
            let report = stopper.join().unwrap()?;
            assert!(elapsed < Duration::from_secs(1));
            let ShutdownOutcome::TimedOut(snapshot) = outcome else {
                panic!("carrier join lost");
            };
            assert_eq!(
                snapshot.shutdown_phase,
                crate::ShutdownPhase::JoiningCarriers
            );
            assert_eq!(runtime.shutdown()?, report);
            let _ = task.join();
            Ok(())
        })
        .unwrap();
}

#[test]
fn owned_threads_can_request_stop_but_cannot_wait_for_themselves() {
    for native in [false, true] {
        let runtime = Arc::new(Runtime::new().unwrap());
        runtime
            .run_scope(|scope| {
                let remote = Arc::clone(&runtime);
                let (checked, receive) = mpsc::sync_channel(1);
                let body = move || {
                    let result = remote.shutdown_until(Instant::now());
                    assert!(matches!(
                        (native, result),
                        (true, Err(Error::InsideManagedWorker))
                            | (false, Err(Error::InsideVThread))
                    ));
                    assert!(remote.snapshot().accepting);
                    remote.request_shutdown();
                    assert!(!remote.snapshot().accepting);
                    checked.send(()).unwrap();
                };
                let mut task = scope.spawn("self-stop", move || {
                    if native {
                        blocking::run(body).ok();
                    } else {
                        body();
                    }
                })?;
                receive.recv_timeout(Duration::from_secs(5)).unwrap();
                runtime.shutdown()?;
                let _ = task.join();
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn zero_active_jobs_do_not_mean_native_thread_local_cleanup_has_finished() {
    struct Capture(mpsc::Receiver<()>, mpsc::SyncSender<()>);
    impl Drop for Capture {
        fn drop(&mut self) {
            self.1.send(()).unwrap();
            let _ = self.0.recv_timeout(Duration::from_secs(5));
        }
    }
    thread_local! {
        static CAPTURE: std::cell::RefCell<Option<Capture>> = const { std::cell::RefCell::new(None) };
    }
    let runtime = Runtime::builder().blocking_threads(1).build().unwrap();
    let (release, gate) = mpsc::sync_channel(1);
    let (started, dropping) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            scope
                .spawn("native-tls", move || {
                    blocking::run(move || {
                        CAPTURE.with(|slot| *slot.borrow_mut() = Some(Capture(gate, started)))
                    })
                })?
                .join()??;
            Ok(())
        })
        .unwrap();
    runtime.request_shutdown();
    dropping.recv_timeout(Duration::from_secs(5)).unwrap();
    let _ = runtime.shutdown_until(Instant::now()).unwrap();
    until(|| runtime.snapshot().shutdown_phase == crate::ShutdownPhase::JoiningNative);
    let outcome = runtime.shutdown_until(Instant::now()).unwrap();
    let _ = release.send(());
    let ShutdownOutcome::TimedOut(snapshot) = outcome else {
        panic!("shutdown completed before OS thread-local cleanup");
    };
    assert_eq!(snapshot.active, 0);
    assert_eq!(snapshot.services.blocking_running, 0);
    assert_eq!(snapshot.shutdown_phase, crate::ShutdownPhase::JoiningNative);
    let mut dump = String::new();
    snapshot.write_dump(&mut dump).unwrap();
    assert!(dump.contains("shutdown=JoiningNative"));
    assert!(matches!(
        runtime.shutdown_until(Instant::now() + Duration::from_secs(5)),
        Ok(ShutdownOutcome::Complete(_))
    ));
}

#[test]
fn final_runtime_owner_on_a_native_worker_does_not_deadlock_the_coordinator() {
    let runtime = Arc::new(Runtime::new().unwrap());
    let retained = Arc::downgrade(&runtime.shared);
    let remote = Arc::clone(&runtime);
    let (release, gate) = mpsc::sync_channel(1);
    let (finished, done) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            let mut task = scope.spawn("last-owner", move || {
                blocking::run(move || {
                    gate.recv_timeout(Duration::from_secs(5)).unwrap();
                    drop(remote);
                    finished.send(()).unwrap();
                })
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            assert!(matches!(
                runtime.shutdown_until(Instant::now())?,
                ShutdownOutcome::TimedOut(_)
            ));
            let _ = task.join();
            Ok(())
        })
        .unwrap();
    drop(runtime);
    release.send(()).unwrap();
    done.recv_timeout(Duration::from_secs(5)).unwrap();
    until(|| retained.upgrade().is_none());
}

#[test]
fn final_runtime_owner_on_a_carrier_is_drained_after_its_destructor_returns() {
    let runtime = Arc::new(Runtime::new().unwrap());
    let retained = Arc::downgrade(&runtime.shared);
    let remote = Arc::clone(&runtime);
    let (release, gate) = mpsc::sync_channel(1);
    let (started, entered) = mpsc::sync_channel(1);
    let supervisor = runtime
        .supervisor_with(crate::ScopeOptions::default())
        .unwrap();
    let mut task = supervisor
        .spawn("last-carrier-owner", move || {
            started.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(remote);
        })
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    // Forgotten supervision does not detach the runtime's ownership of the started stack.
    std::mem::forget(supervisor);
    assert!(matches!(
        runtime.shutdown_until(Instant::now()).unwrap(),
        ShutdownOutcome::TimedOut(_)
    ));
    drop(runtime);
    release.send(()).unwrap();
    let _ = task.join();
    drop(task);
    until(|| retained.upgrade().is_none());
}
