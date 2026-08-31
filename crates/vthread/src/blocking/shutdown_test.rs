use crate::{Runtime, ShutdownOutcome, blocking, support_test::until};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

#[test]
fn stop_request_does_not_execute_queued_user_destructors_on_its_caller() {
    struct Capture(mpsc::Receiver<()>, mpsc::SyncSender<thread::ThreadId>);
    impl Drop for Capture {
        fn drop(&mut self) {
            self.1.send(thread::current().id()).unwrap();
            self.0.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }
    let runtime = Arc::new(Runtime::builder().blocking_threads(1).build().unwrap());
    let (release_job, job_gate) = mpsc::sync_channel(1);
    let (release_drop, drop_gate) = mpsc::sync_channel(1);
    let (drop_thread, dropped) = mpsc::sync_channel(1);
    let (stopped, stop_done) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            let mut first = scope.spawn("running", move || {
                blocking::run(move || job_gate.recv_timeout(Duration::from_secs(5)).unwrap())
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            let capture = Capture(drop_gate, drop_thread);
            let mut second = scope.spawn("queued", move || {
                blocking::run(move || {
                    let _capture = capture;
                    panic!("a stopped queued body must never run");
                })
            })?;
            until(|| runtime.snapshot().services.blocking_queued == 1);
            let remote = Arc::clone(&runtime);
            let stopper = thread::spawn(move || {
                remote.shared.request_stop();
                stopped.send(()).unwrap();
            });
            let stopper_id = stopper.thread().id();
            let returned = stop_done.recv_timeout(Duration::from_millis(100)).is_ok();
            // Release both gates before asserting so the old implementation also drains.
            release_job.send(()).unwrap();
            release_drop.send(()).unwrap();
            stopper.join().unwrap();
            runtime.shutdown()?;
            let _ = first.join();
            let _ = second.join();
            assert!(
                returned,
                "stop request blocked inside a queued capture destructor"
            );
            assert_ne!(dropped.recv().unwrap(), stopper_id);
            Ok(())
        })
        .unwrap();
}

#[test]
fn timed_shutdown_retains_blocked_cleanup_and_isolates_its_panic() {
    struct Capture(mpsc::Receiver<()>, mpsc::SyncSender<()>, Arc<Runtime>);
    impl Drop for Capture {
        fn drop(&mut self) {
            assert_eq!(self.2.snapshot().services.blocking_discarding, 1);
            assert!(matches!(
                self.2.shutdown(),
                Err(crate::Error::InsideManagedWorker)
            ));
            self.1.send(()).unwrap();
            self.0.recv_timeout(Duration::from_secs(5)).unwrap();
            panic!("queued cleanup panic");
        }
    }
    let runtime = Arc::new(Runtime::builder().blocking_threads(1).build().unwrap());
    let (release_job, job_gate) = mpsc::sync_channel(1);
    let (release_drop, drop_gate) = mpsc::sync_channel(1);
    let (drop_started, dropping) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            let first = scope.spawn("running", move || {
                blocking::run(move || job_gate.recv_timeout(Duration::from_secs(5)).unwrap())
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            let capture = Capture(drop_gate, drop_started, Arc::clone(&runtime));
            let second = scope.spawn("queued", move || blocking::run(move || drop(capture)))?;
            until(|| runtime.snapshot().services.blocking_queued == 1);
            let third = scope.spawn("later", || blocking::run(|| panic!("stopped body")))?;
            until(|| runtime.snapshot().services.blocking_queued == 2);
            // Stop the pool before releasing its running job; no queued body can start.
            runtime.shared.services.get().unwrap().blocking.stop();
            runtime.request_shutdown();
            release_job.send(()).unwrap();
            dropping.recv_timeout(Duration::from_secs(5)).unwrap();
            let outcome = runtime.shutdown_until(Instant::now())?;
            release_drop.send(()).unwrap();
            let ShutdownOutcome::TimedOut(snapshot) = outcome else {
                panic!("capture destructor was not joined");
            };
            assert_eq!(snapshot.services.blocking_discarding, 1);
            assert_eq!(snapshot.services.blocking_queued, 1);
            assert_eq!(snapshot.services.blocking_running, 0);
            let mut dump = String::new();
            snapshot.write_dump(&mut dump).unwrap();
            assert!(dump.contains("blocking_discarding=1"));
            assert!(dump.contains("accepting=false"));
            runtime.shutdown()?;
            for mut child in [first, second, third] {
                let _ = child.join();
            }
            let services = runtime.snapshot().services;
            assert_eq!(services.blocking_panics, 1);
            assert_eq!(services.blocking_discarding + services.blocking_queued, 0);
            Ok(())
        })
        .unwrap();
}

#[test]
fn timed_shutdown_does_not_wait_behind_a_native_worker_join() {
    let runtime = Arc::new(Runtime::new().unwrap());
    let (release, gate) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            let mut task = scope.spawn("native", move || {
                blocking::run(move || gate.recv_timeout(Duration::from_secs(5)).unwrap())
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            let remote = Arc::clone(&runtime);
            let stopper = thread::spawn(move || remote.shutdown());
            until(|| runtime.snapshot().shutdown_phase == crate::ShutdownPhase::JoiningNative);
            let before = Instant::now();
            let outcome = runtime.shutdown_until(before + Duration::from_millis(20))?;
            let elapsed = before.elapsed();
            release.send(()).unwrap();
            stopper.join().unwrap()?;
            assert!(elapsed < Duration::from_secs(1));
            assert!(matches!(outcome, ShutdownOutcome::TimedOut(_)));
            let _ = task.join();
            Ok(())
        })
        .unwrap();
}
