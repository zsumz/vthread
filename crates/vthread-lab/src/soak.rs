//! Continuous mixed traffic with bounded batches and reclamation assertions.

use std::time::{Duration, Instant};
use vthread::{
    Result, Runtime, ScopeOptions, diagnostics::RuntimeStats, diagnostics::StackSnapshot,
};

pub(crate) struct Report {
    pub iterations: u64,
    pub mutex_updates: u64,
    pub elapsed: Duration,
    pub stats: RuntimeStats,
    pub stacks: StackSnapshot,
}

pub(crate) fn run(duration: Duration, carriers: usize, tasks: usize) -> Result<Report> {
    let runtime = Runtime::builder()
        .carriers(carriers)
        .max_vthreads(tasks + 8)
        .carrier_queue_capacity(tasks + 8)
        .io_capacity(tasks + 8)
        .blocking_capacity(tasks + 8)
        .stack_cache_capacity(tasks + 8)
        .build()?;
    let start = Instant::now();
    let mut iterations = 0;
    let mut network = None;
    while start.elapsed() < duration {
        let options = ScopeOptions::default().deadline(Instant::now() + Duration::from_secs(10));
        network = Some(runtime.run_scope_with(options, |scope| {
            super::workload::batch(scope, tasks, iterations, network.take())
        })?);
        super::workload::cancel(&runtime)?;
        let drain_deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = runtime.snapshot();
            let services = &snapshot.services();
            if services_drained(services) {
                assert_eq!(snapshot.active(), 0);
                assert_eq!(services.readiness_waits(), 0);
                assert_eq!(services.blocking_queued(), 0);
                assert_eq!(services.blocking_discarding(), 0);
                assert!(!services.readiness_failed());
                assert_eq!(services.blocking_panics(), 0);
                assert!(snapshot.last_stall().is_none());
                assert!(
                    snapshot
                        .carriers()
                        .iter()
                        .all(|c| c.stacks().cached() <= tasks + 8)
                );
                break;
            }
            if Instant::now() >= drain_deadline {
                let mut dump = String::new();
                snapshot.write_dump(&mut dump).expect("string dump");
                panic!("services did not drain: {dump}");
            }
            std::thread::yield_now();
        }
        iterations += 1;
    }
    runtime.shutdown()?;
    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.active(), 0);
    assert_eq!(snapshot.services().readiness_waits(), 0);
    assert_eq!(snapshot.services().readiness_registered(), 0);
    assert_eq!(snapshot.services().blocking_queued(), 0);
    assert_eq!(snapshot.services().blocking_completed(), 0);
    assert_eq!(snapshot.services().blocking_running(), 0);
    assert_eq!(snapshot.services().blocking_discarding(), 0);
    assert!(!snapshot.accepting());
    Ok(Report {
        iterations,
        mutex_updates: iterations * tasks as u64 * super::workload::MUTEX_UPDATES_PER_TASK as u64,
        elapsed: start.elapsed(),
        stats: snapshot.stats(),
        stacks: snapshot.stacks(),
    })
}

fn services_drained(services: &vthread::diagnostics::ServiceSnapshot) -> bool {
    services.readiness_waits() == 0
        && services.readiness_registered() == 0
        && services.blocking_queued() == 0
        && services.blocking_running() == 0
        && services.blocking_completed() == 0
        && services.blocking_discarding() == 0
}

#[cfg(test)]
#[path = "soak_test.rs"]
mod soak_test;
