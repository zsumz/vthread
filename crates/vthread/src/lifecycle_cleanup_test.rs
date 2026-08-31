use crate::{Error, Runtime, signal::lock};
use std::{
    sync::{atomic::Ordering, mpsc},
    thread,
    time::Duration,
};

struct Capture(mpsc::SyncSender<String>);
impl Drop for Capture {
    fn drop(&mut self) {
        let name = thread::current().name().unwrap_or("unnamed").to_owned();
        let _ = self.0.send(name.clone());
        if name == "vthread-lifecycle-owner" {
            let (_hold, blocked) = mpsc::sync_channel::<()>(1);
            let _ = blocked.recv_timeout(Duration::from_secs(30));
        }
    }
}

#[test]
fn incomplete_carrier_abort_and_retirement_remain_process_owned() {
    const CHILD: &str = "VTHREAD_PARTIAL_CARRIER_CHILD";
    let Ok(phase) = std::env::var(CHILD) else {
        for phase in [1, 2] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "lifecycle_owner::lifecycle_cleanup_test::incomplete_carrier_abort_and_retirement_remain_process_owned", "--nocapture"])
                .env(CHILD, phase.to_string()).output().unwrap();
            assert!(
                output.status.success(),
                "phase{phase}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    };
    let phase: usize = phase.parse().unwrap();
    let (finished, watchdog) = mpsc::sync_channel(1);
    let watcher = thread::spawn(move || {
        if watchdog.recv_timeout(Duration::from_secs(5)).is_err() {
            std::process::exit(29);
        }
    });
    let runtime = Runtime::new().unwrap();
    let id = runtime.id();
    let scope = runtime.shared.begin_scope().unwrap();
    let (dropped, observed) = mpsc::sync_channel(5);
    let native_release = if phase == 1 {
        let (release, gate) = mpsc::sync_channel(1);
        let capture = Capture(dropped.clone());
        runtime
            .shared
            .submit(scope, "retained-native-result".into(), move || {
                crate::blocking::run(move || {
                    gate.recv_timeout(Duration::from_secs(3)).unwrap();
                    capture
                })
            })
            .unwrap();
        crate::support_test::until(|| runtime.snapshot().services.blocking_running == 1);
        Some(release)
    } else {
        None
    };
    let (release, gate) = mpsc::sync_channel(1);
    let (entered, started) = mpsc::sync_channel(1);
    runtime
        .shared
        .submit(scope, "hold-carrier".into(), move || {
            entered.send(()).unwrap();
            gate.recv_timeout(Duration::from_secs(3)).unwrap();
        })
        .unwrap();
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    if let Some(release) = native_release {
        release.send(()).unwrap();
        crate::support_test::until(|| runtime.snapshot().services.blocking_completed == 1);
    }
    for _ in 0..3 {
        let capture = Capture(dropped.clone());
        runtime
            .shared
            .submit(scope, "queued-capture".into(), move || drop(capture))
            .unwrap();
    }
    runtime.shared.carrier_fault.store(phase, Ordering::Release);
    runtime.request_shutdown();
    release.send(()).unwrap();
    assert!(matches!(runtime.shutdown(), Err(Error::LifecycleFailed(_))));
    drop(runtime);
    assert!(matches!(Runtime::new(), Err(Error::LifecycleFailed(_))));
    let owner = lock(super::OWNER.get().unwrap());
    let slots = lock(&owner.as_ref().unwrap().state.slots);
    let retained = slots
        .entries
        .iter()
        .find(|entry| entry.shared.id == id)
        .unwrap();
    assert!(retained.worker.joined());
    assert_eq!(retained.resources.workers.counts(), (1, 1));
    assert!(retained.resources.returned.load(Ordering::Acquire));
    assert!(!retained.resources.drained(&retained.shared, true));
    assert!(
        retained.shared.inboxes[0]
            .scheduler_stopped
            .load(Ordering::Acquire)
    );
    assert!(!retained.shared.inboxes[0].reclaimed.load(Ordering::Acquire));
    assert_eq!(
        retained.shared.snapshot().carriers[0].status,
        crate::CarrierStatus::Failed
    );
    assert_eq!(
        retained.shared.inboxes[0].pending(),
        usize::from(phase == 1)
    );
    // Services are retained before joining them: a leaked affine stack could own
    // a native completion acknowledgement forever.
    assert_eq!(retained.resources.native.get().unwrap().counts(), (2, 0));
    if phase == 1 {
        assert_eq!(retained.shared.snapshot().services.blocking_completed, 1);
    }
    let drops = observed.try_iter().collect::<Vec<_>>();
    assert_eq!(drops.len(), if phase == 1 { 2 } else { 3 });
    assert!(
        drops
            .iter()
            .all(|name| name.starts_with("vthread-carrier-"))
    );
    finished.send(()).unwrap();
    watcher.join().unwrap();
}
