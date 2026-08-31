use crate::{Error, Runtime, ShutdownPhase, signal::lock};
use std::{
    sync::{Arc, atomic::Ordering, mpsc},
    thread,
    time::Duration,
};

fn isolated(name: &str) -> bool {
    const CHILD: &str = "VTHREAD_TERMINAL_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("lifecycle_owner::lifecycle_terminal_test::{name}"),
            "--nocapture",
        ])
        .env(CHILD, name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    true
}

fn state() -> Arc<super::State> {
    Arc::clone(&lock(super::OWNER.get().unwrap()).as_ref().unwrap().state)
}

#[test]
fn terminal_shutdown_releases_capacity_before_replacement_construction() {
    if isolated("terminal_shutdown_releases_capacity_before_replacement_construction") {
        return;
    }
    for failed in [false, true] {
        let first = Arc::new(Runtime::new().unwrap());
        if failed {
            *lock(&first.shared.carrier_exit_hook) =
                Some(Box::new(|| panic!("joined carrier failure")));
        }
        let state = state();
        state.capacity.store(2, Ordering::Release);
        let second = Runtime::new().unwrap();
        assert!(matches!(
            Runtime::new(),
            Err(Error::Capacity {
                resource: crate::error::CapacityResource::Lifecycles,
                limit: 2
            })
        ));
        let (reached, boundary) = mpsc::sync_channel(1);
        let (release, proceed) = mpsc::sync_channel(1);
        *lock(&state.terminal_hook) = Some(Box::new(move || {
            reached.send(()).unwrap();
            proceed.recv_timeout(Duration::from_secs(5)).unwrap();
        }));
        let waiting = Arc::clone(&first);
        let waiter = thread::spawn(move || waiting.shutdown());
        boundary.recv_timeout(Duration::from_secs(5)).unwrap();
        let occupied = lock(&state.slots).occupied;
        let phase = first.snapshot().shutdown_phase();
        release.send(()).unwrap();
        let outcome = waiter.join().unwrap();
        assert!(matches!(
            (failed, outcome),
            (false, Ok(_)) | (true, Err(Error::ShutdownFailed(_)))
        ));
        let replacement = Runtime::new().unwrap();
        assert_eq!(
            occupied, 1,
            "terminal publication preceded capacity release"
        );
        assert!(
            phase < ShutdownPhase::Complete,
            "terminal success published before final commit"
        );
        replacement.shutdown().unwrap();
        second.shutdown().unwrap();
    }
}

#[test]
fn failure_at_terminal_commit_cannot_publish_success() {
    if isolated("failure_at_terminal_commit_cannot_publish_success") {
        return;
    }
    let runtime = Runtime::new().unwrap();
    let state = state();
    state.fail_at.store(4, Ordering::Release);
    let result = runtime.shutdown();
    crate::support_test::until(|| {
        matches!(super::lifecycle_health(), super::LifecycleHealth::Failed(_))
    });
    assert!(
        matches!(result, Err(Error::LifecycleFailed(_))),
        "premature result: {result:?}"
    );
    assert_ne!(runtime.snapshot().shutdown_phase(), ShutdownPhase::Complete);
    assert!(matches!(Runtime::new(), Err(Error::LifecycleFailed(_))));
}
