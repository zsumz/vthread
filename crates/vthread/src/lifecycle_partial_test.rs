use crate::{Error, Runtime, signal::lock};
use std::sync::{atomic::Ordering, mpsc};
use std::{thread, time::Duration};

#[test]
fn claimed_carrier_handle_survives_coordinator_unwind() {
    const CHILD: &str = "VTHREAD_PARTIAL_DRAIN_CHILD";
    let Ok(phase) = std::env::var(CHILD) else {
        for phase in 1..=6 {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "lifecycle_owner::lifecycle_partial_test::claimed_carrier_handle_survives_coordinator_unwind", "--nocapture"])
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
            std::process::exit(28);
        }
    });
    let runtime = Runtime::builder()
        .carriers(2)
        .blocking_threads(2)
        .build()
        .unwrap();
    let id = runtime.id();
    runtime
        .shared
        .coordinator_fault
        .store(phase, Ordering::Relaxed);
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
    let carrier_joined = match phase {
        1 => 0,
        2 | 6 => 1,
        _ => 2,
    };
    assert_eq!(
        retained.resources.workers.counts(),
        (2, carrier_joined),
        "carrier handle lost"
    );
    assert_eq!(
        retained.resources.readiness.get().unwrap().counts(),
        (1, usize::from(matches!(phase, 4 | 5)))
    );
    assert_eq!(
        retained.resources.native.get().unwrap().counts(),
        (2, usize::from(phase == 5))
    );
    assert!(!retained.resources.drained(&retained.shared, true));
    finished.send(()).unwrap();
    watcher.join().unwrap();
}
