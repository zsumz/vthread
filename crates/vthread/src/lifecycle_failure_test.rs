use crate::{Error, Runtime, signal::lock};
use std::{
    sync::{atomic::Ordering, mpsc},
    thread,
    time::Duration,
};

#[test]
fn premature_coordinator_exit_retains_undrained_runtime_ownership() {
    const CHILD: &str = "VTHREAD_UNDRAINED_COORDINATOR_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "lifecycle_owner::lifecycle_failure_test::premature_coordinator_exit_retains_undrained_runtime_ownership", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert!(
            output.status.success(),
            "isolated coordinator failure: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    let (finished, watchdog) = mpsc::sync_channel(1);
    let watcher = thread::spawn(move || {
        if watchdog.recv_timeout(Duration::from_secs(3)).is_err() {
            std::process::exit(26);
        }
    });
    let runtime = Runtime::new().unwrap();
    let id = runtime.id();
    runtime
        .shared
        .fail_coordinator_before_drain
        .store(true, Ordering::Relaxed);
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
    assert!(
        retained.worker.is_none(),
        "coordinator exit itself was not joined"
    );
    assert!(!retained.resources.drained.load(Ordering::Acquire));
    assert_eq!(
        lock(&retained.resources.workers).len(),
        1,
        "carrier join handles were dropped during coordinator unwind"
    );
    assert_ne!(
        retained.shared.shutdown_phase(),
        crate::ShutdownPhase::Complete
    );
    finished.send(()).unwrap();
    watcher.join().unwrap();
}
