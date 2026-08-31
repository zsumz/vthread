use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{Runtime, sleep};

#[test]
fn sleeping_parks_instead_of_blocking_the_next_task() {
    let runtime = Runtime::new().expect("build runtime");
    let trace = Arc::new(Mutex::new(Vec::new()));

    runtime
        .run_scope(|scope| {
            let (release, wait) = std::sync::mpsc::sync_channel(1);
            let mut gate = scope.spawn("admission-gate", move || {
                wait.recv_timeout(Duration::from_secs(5))
                    .expect("release queued tasks");
            })?;
            let sleeper_trace = Arc::clone(&trace);
            let mut sleeper = scope.spawn("sleeper", move || {
                sleeper_trace.lock().expect("trace").push("sleep:start");
                sleep(Duration::from_millis(1)).expect("sleep task");
                sleeper_trace.lock().expect("trace").push("sleep:end");
            })?;
            let worker_trace = Arc::clone(&trace);
            let mut worker = scope.spawn("worker", move || {
                worker_trace.lock().expect("trace").push("worker");
            })?;

            release.send(()).expect("release carrier");
            gate.join()?;
            sleeper.join()?;
            worker.join()?;
            Ok(())
        })
        .expect("scope succeeds");

    assert_eq!(
        &*trace.lock().expect("trace"),
        &["sleep:start", "worker", "sleep:end"]
    );
    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.stats.parks, 1);
    assert_eq!(snapshot.stats.timeouts, 1);
}
