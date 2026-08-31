#[test]
fn mixed_traffic_finishes_and_reclaims_all_services() {
    for carriers in [1, 4] {
        let report = super::run(std::time::Duration::from_millis(20), carriers, 4).unwrap();
        assert!(report.iterations > 0);
        assert!(report.stats.completed() >= 8);
        assert!(report.stats.parks() > 0);
    }
}

#[test]
fn blocked_native_disposal_is_not_drained() {
    use std::{sync::mpsc, time::Duration};
    use vthread::{Error, Runtime, blocking};
    struct Capture(mpsc::SyncSender<()>, mpsc::Receiver<()>);
    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.send(()).unwrap();
            self.1.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }
    let runtime = Runtime::builder()
        .carriers(1)
        .blocking_threads(1)
        .build()
        .unwrap();
    let (release_job, job_gate) = mpsc::sync_channel(1);
    let (release_drop, drop_gate) = mpsc::sync_channel(1);
    let (drop_started, dropping) = mpsc::sync_channel(1);
    let observed = runtime
        .run_scope(|scope| {
            let mut first = scope.spawn("occupied-native", move || {
                blocking::run(move || job_gate.recv_timeout(Duration::from_secs(5)).unwrap())
            })?;
            until(|| runtime.snapshot().services().blocking_running() == 1);
            let capture = Capture(drop_started, drop_gate);
            let mut queued = scope.spawn("queued-native-capture", move || {
                blocking::run(move || {
                    let _capture = capture;
                    panic!("cancelled queued body executed");
                })
            })?;
            until(|| runtime.snapshot().services().blocking_queued() == 1);
            scope.cancel();
            assert!(matches!(first.join()?, Err(Error::Cancelled)));
            assert!(matches!(queued.join()?, Err(Error::Cancelled)));
            release_job.send(()).unwrap();
            dropping.recv_timeout(Duration::from_secs(5)).unwrap();
            let snapshot = runtime.snapshot();
            let services = snapshot.services();
            let observed = (
                services.blocking_running(),
                services.blocking_discarding(),
                super::services_drained(services),
            );
            // Always release the destructor before reporting a regression failure.
            release_drop.send(()).unwrap();
            Ok(observed)
        })
        .unwrap();
    runtime.shutdown().unwrap();
    assert_eq!(observed.0, 0);
    assert_eq!(observed.1, 1);
    assert!(
        !observed.2,
        "a blocked native destructor still owns capacity"
    );
}

#[test]
fn retained_completed_native_result_is_not_drained() {
    use std::{sync::mpsc, time::Duration};
    use vthread::{Runtime, blocking};
    let runtime = Runtime::builder()
        .carriers(1)
        .blocking_threads(1)
        .build()
        .unwrap();
    let (release_job, job_gate) = mpsc::sync_channel(1);
    let (release_carrier, carrier_gate) = mpsc::sync_channel(1);
    let (blocked, carrier_blocked) = mpsc::sync_channel(1);
    let observed = runtime
        .run_scope(|scope| {
            let mut native = scope.spawn("completed-native", move || {
                blocking::run(move || {
                    job_gate.recv_timeout(Duration::from_secs(5)).unwrap();
                    String::from("retained result")
                })
            })?;
            until(|| runtime.snapshot().services().blocking_running() == 1);
            let mut barrier = scope.spawn("hold-commit", move || {
                blocked.send(()).unwrap();
                carrier_gate.recv_timeout(Duration::from_secs(5)).unwrap();
            })?;
            carrier_blocked
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            release_job.send(()).unwrap();
            until(|| runtime.snapshot().services().blocking_completed() == 1);
            let snapshot = runtime.snapshot();
            let services = snapshot.services();
            let observed = (
                services.blocking_running(),
                services.blocking_completed(),
                super::services_drained(services),
            );
            release_carrier.send(()).unwrap();
            barrier.join()?;
            assert_eq!(native.join()??, "retained result");
            Ok(observed)
        })
        .unwrap();
    runtime.shutdown().unwrap();
    assert_eq!(observed.0, 0);
    assert_eq!(observed.1, 1);
    assert!(
        !observed.2,
        "an uncommitted native result still owns capacity"
    );
}

fn until(mut condition: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        assert!(
            std::time::Instant::now() < deadline,
            "fixture synchronization timeout"
        );
        std::thread::yield_now();
    }
}
