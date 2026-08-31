use crate::{Error, Runtime, blocking, support_test::until};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

struct Capture {
    dropped: mpsc::SyncSender<String>,
    gate: Option<mpsc::Receiver<()>>,
    panic: bool,
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.dropped
            .send(thread::current().name().unwrap_or("unnamed").to_owned())
            .unwrap();
        if let Some(gate) = &self.gate {
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        assert!(!self.panic, "queued capture cleanup panic");
    }
}

fn queued_capture(blocks: bool, panics: bool) {
    let runtime = Runtime::builder()
        .blocking_threads(1)
        .blocking_capacity(2)
        .build()
        .unwrap();
    let (release_job, job_gate) = mpsc::sync_channel(1);
    let (release_drop, drop_gate) = mpsc::sync_channel(1);
    let (dropped, thread_name) = mpsc::sync_channel(1);
    let (returned, return_seen) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            let mut first = scope.spawn("occupied-worker", move || {
                blocking::run(move || job_gate.recv_timeout(Duration::from_secs(5)).unwrap())
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            let capture = Capture {
                dropped,
                gate: blocks.then_some(drop_gate),
                panic: panics,
            };
            let mut queued = scope.spawn("cancelled-capture", move || {
                let result = blocking::run(move || {
                    let _capture = capture;
                    panic!("cancelled queued body executed");
                });
                let _ = returned.send(());
                result
            })?;
            until(|| runtime.snapshot().services.blocking_queued == 1);
            scope.cancel();
            // The watchdog lets the old carrier-destructor implementation drain too.
            let returned_before_worker =
                return_seen.recv_timeout(Duration::from_millis(500)).is_ok();
            let queued_before_worker = runtime.snapshot().services.blocking_queued;
            release_job.send(()).unwrap();
            let destructor_thread = thread_name.recv_timeout(Duration::from_secs(5)).unwrap();
            let discarding = runtime.snapshot().services.blocking_discarding;
            if blocks {
                release_drop.send(()).unwrap();
            }
            let outcome = queued.join();
            let _ = first.join();
            assert!(
                returned_before_worker,
                "cancellation ran a capture destructor"
            );
            assert_eq!(
                queued_before_worker, 1,
                "native pool must retain the tombstone"
            );
            assert!(
                destructor_thread.starts_with("vthread-blocking-"),
                "{destructor_thread}"
            );
            if blocks {
                assert_eq!(discarding, 1, "capacity must cover native destruction");
            }
            assert!(matches!(outcome, Ok(Err(Error::Cancelled))));
            Ok(())
        })
        .unwrap();
    runtime.shutdown().unwrap();
    assert_eq!(
        runtime.snapshot().services.blocking_panics,
        u64::from(panics)
    );
}

#[test]
fn queued_capture_cancellation_reclaims_on_a_native_worker() {
    queued_capture(false, false);
}

#[test]
fn blocking_queued_capture_drop_retains_native_capacity() {
    queued_capture(true, false);
}

#[test]
fn panicking_queued_capture_drop_is_isolated_from_cancellation() {
    queued_capture(false, true);
}

fn completed_result(cancel: bool) {
    let runtime = Arc::new(
        Runtime::builder()
            .carriers(1)
            .blocking_threads(1)
            .blocking_capacity(1)
            .build()
            .unwrap(),
    );
    let (release_job, job_gate) = mpsc::sync_channel(1);
    let (release_carrier, carrier_gate) = mpsc::sync_channel(1);
    let (blocked, carrier_blocked) = mpsc::sync_channel(1);
    let (dropped, thread_name) = mpsc::sync_channel(1);
    runtime
        .run_scope(|scope| {
            let mut call = scope.spawn("completed-native", move || {
                blocking::run(move || {
                    job_gate.recv_timeout(Duration::from_secs(5)).unwrap();
                    Capture {
                        dropped,
                        gate: None,
                        panic: false,
                    }
                })
            })?;
            until(|| runtime.snapshot().services.blocking_running == 1);
            let mut barrier = scope.spawn("hold-carrier", move || {
                blocked.send(()).unwrap();
                carrier_gate.recv_timeout(Duration::from_secs(5)).unwrap();
            })?;
            carrier_blocked
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            release_job.send(()).unwrap();
            until(|| runtime.snapshot().services.blocking_running == 0);
            assert_eq!(runtime.snapshot().services.blocking_completed(), 1);
            let cell = crate::wait::WaitCell::new();
            let rejected = runtime.shared.services.get().unwrap().blocking.submit(
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
                vthread_stack::ParkToken::new(cell.identity(), 1),
                cell.registration(),
                Box::new(|| false),
                Box::new(|| {}),
            );
            assert!(matches!(
                rejected,
                Err(Error::Capacity {
                    resource: crate::error::CapacityResource::NativeJobs,
                    limit: 1
                })
            ));
            if cancel {
                scope.cancel();
            } else {
                runtime.request_shutdown();
            }
            release_carrier.send(()).unwrap();
            let _ = barrier.join();
            let result = call.join();
            if cancel {
                assert!(matches!(result, Ok(Ok(_))), "Ready must commit its result");
                drop(result);
                assert_eq!(
                    thread_name.recv().unwrap(),
                    thread::current().name().unwrap()
                );
            } else {
                runtime.shutdown()?;
                assert!(matches!(result, Err(Error::TaskAborted { .. })));
                let destructor_thread = thread_name.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(
                    destructor_thread.starts_with("vthread-blocking-"),
                    "{destructor_thread}"
                );
            }
            Ok(())
        })
        .unwrap();
    runtime.shutdown().unwrap();
}

#[test]
fn completed_ready_result_commits_before_later_cancellation() {
    completed_result(true);
}

#[test]
fn completed_ready_result_is_reclaimed_natively_after_forced_abort() {
    completed_result(false);
}
