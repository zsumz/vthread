//! Preserve progress evidence before shutdown can turn a stall into rejection.

use crate::{CarrierId, Error, RuntimeConfig, control::Shared};
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const TASKS: usize = 4_096;
const WATCHDOG: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Counters {
    accepted: AtomicUsize,
    started: AtomicUsize,
    returned: AtomicUsize,
    cleanup: AtomicBool,
}

struct Stop(Arc<Shared>, Arc<Counters>);

impl Drop for Stop {
    fn drop(&mut self) {
        self.1.cleanup.store(true, Ordering::SeqCst);
        self.0.request_stop();
    }
}

#[derive(Debug, PartialEq, Eq)]
struct BeforeStop {
    accepted_begin: usize,
    accepted_end: usize,
    queued: usize,
    started: usize,
    body_returns: usize,
    completed_credits: u64,
    active: usize,
}

fn observe(shared: &Shared, scope: u64, counters: &Counters) -> BeforeStop {
    assert!(
        !counters.cleanup.load(Ordering::SeqCst),
        "progress evidence captured after cleanup"
    );
    let accepted_begin = counters.accepted.load(Ordering::SeqCst);
    let snapshot = shared.snapshot();
    let report = shared.scope_report(scope);
    let before = BeforeStop {
        accepted_begin,
        accepted_end: counters.accepted.load(Ordering::SeqCst),
        queued: shared.inboxes[0].pending(),
        started: counters.started.load(Ordering::SeqCst),
        body_returns: counters.returned.load(Ordering::SeqCst),
        completed_credits: report.completed,
        active: snapshot.active,
    };
    // Concurrent observations are a window, not one atomic snapshot. Published
    // carrier counters may lag; scope credits are distinct from body returns.
    writeln!(
        std::io::stdout().lock(),
        "refill-before-stop progress={before:?} accepting={} epoch={} waiting={} \
         scope={report:?} carriers={:?}",
        snapshot.accepting,
        shared.inboxes[0].signal.version(),
        shared.inboxes[0].signal.waiting(),
        snapshot.carriers
    )
    .unwrap();
    before
}

fn fill(shared: &Shared, scope: u64, counters: &Arc<Counters>) -> crate::Result<()> {
    for index in 0..TASKS {
        loop {
            if counters.cleanup.load(Ordering::SeqCst) {
                return Err(Error::RuntimeStopped);
            }
            let body = Arc::clone(counters);
            match shared.submit(scope, format!("refill-{index}"), move || {
                body.started.fetch_add(1, Ordering::SeqCst);
                body.returned.fetch_add(1, Ordering::SeqCst);
            }) {
                Ok(_) => {
                    counters.accepted.fetch_add(1, Ordering::SeqCst);
                    break;
                }
                Err(Error::Capacity {
                    resource: crate::error::CapacityResource::CarrierQueue,
                    ..
                }) => thread::yield_now(),
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

#[test]
fn continuously_refilled_coalesced_inbox_is_fully_drained() {
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let counters = Arc::new(Counters::default());
    let outcome = thread::scope(|threads| {
        // Stop is dropped before scoped joins, including every observer failure.
        let stop = Stop(Arc::clone(&shared), Arc::clone(&counters));
        let worker_shared = Arc::clone(&shared);
        let worker = threads.spawn(move || super::run(worker_shared, CarrierId(0)));
        let startup_deadline = Instant::now() + WATCHDOG;
        while shared.inboxes[0].signal.waiting() == 0 && Instant::now() < startup_deadline {
            thread::yield_now();
        }
        if shared.inboxes[0].signal.waiting() == 0 {
            let before = observe(&shared, scope, &counters);
            panic!("initial carrier wait was not observed: {before:?}");
        }
        // Admission now necessarily exercises the initial sleeping-owner signal.
        let producer_shared = Arc::clone(&shared);
        let produced = Arc::clone(&counters);
        let (submitted, submitted_rx) = mpsc::sync_channel(1);
        let producer = threads.spawn(move || {
            let result = fill(&producer_shared, scope, &produced);
            let _ = submitted.send(result);
        });
        let admission = submitted_rx.recv_timeout(WATCHDOG);
        let drained = matches!(admission, Ok(Ok(())))
            .then(|| shared.wait_until(scope, None, Some(Instant::now() + WATCHDOG)));
        let before = observe(&shared, scope, &counters);
        writeln!(
            std::io::stdout().lock(),
            "refill-primary admission={admission:?} drained={drained:?}"
        )
        .unwrap();
        drop(stop);
        let producer = producer.join().map_err(crate::PanicReport::capture);
        let late_admission = admission.is_err().then(|| submitted_rx.try_recv());
        let worker = worker.join().map_err(crate::PanicReport::capture);
        writeln!(std::io::stdout().lock(),
            "refill-cleanup producer={producer:?} worker={worker:?} late_admission={late_admission:?}").unwrap();
        (admission, drained, before, producer, worker)
    });
    let cleanup = shared.scope_report(scope);
    writeln!(std::io::stdout().lock(), "refill-final cleanup={cleanup:?}").unwrap();
    shared.finish_scope(scope);
    let (admission, drained, before, producer, worker) = outcome;
    admission
        .expect("continuous refill admission gate failed")
        .expect("admission returned a runtime error");
    assert!(
        matches!(drained, Some(Ok(true))),
        "accepted work did not drain: {drained:?}"
    );
    assert_eq!(
        before,
        BeforeStop {
            accepted_begin: TASKS,
            accepted_end: TASKS,
            queued: 0,
            started: TASKS,
            body_returns: TASKS,
            completed_credits: TASKS as u64,
            active: 0,
        }
    );
    producer.expect("producer panic retained after primary observation");
    worker.expect("carrier panic retained after primary observation");
    assert!(cleanup.failures.is_empty(), "{cleanup:?}");
    assert_eq!(
        (cleanup.aborted, cleanup.panicked, cleanup.failed_carriers),
        (0, 0, 0)
    );
    assert!(shared.inboxes[0].reclaimed.load(Ordering::Acquire));
}
