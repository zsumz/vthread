//! Preserve progress evidence before shutdown can turn a stall into rejection.
use crate::support_test::{
    RefillBeforeStop as BeforeStop, RefillCounters as Counters, install_admission_progress,
    observe_refill_passive, observe_refill_rich, run_isolated, wait_without_intervention,
};
use crate::{CarrierId, Error, RuntimeConfig, control::Shared};
use std::{
    io::Write,
    sync::{Arc, atomic::Ordering, mpsc},
    thread,
    time::{Duration, Instant},
};

const TASKS: usize = 4_096;
const WATCHDOG: Duration = Duration::from_secs(5);

struct Stop(Arc<Shared>, Arc<Counters>);

impl Drop for Stop {
    fn drop(&mut self) {
        self.1.cleanup.store(true, Ordering::SeqCst);
        self.0.request_stop();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Primary {
    Passed,
    Admission,
    Drain,
}

fn assert_complete(before: BeforeStop, tasks: usize, counters: &Counters) {
    assert_eq!(
        before,
        BeforeStop {
            accepted_begin: tasks,
            accepted_end: tasks,
            queued: 0,
            started: tasks,
            body_returns: tasks,
            completed_credits: tasks as u64,
            active: 0,
        }
    );
    counters.admission.assert_successful(tasks);
}

fn isolate(name: &str) -> bool {
    if std::env::var("VTHREAD_REFILL_CHILD").as_deref() == Ok(name) {
        return false;
    }
    let exact = format!("carrier::carrier_refill_test::{name}");
    let output = run_isolated(&exact, ("VTHREAD_REFILL_CHILD", name), WATCHDOG * 5);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.timed_out && output.status.success() && stdout.contains("1 passed"),
        "isolated refill failed: status={:?} timed_out={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status,
        output.timed_out,
    );
    true
}

fn fill(shared: &Shared, scope: u64, counters: &Arc<Counters>) -> crate::Result<()> {
    let _progress = install_admission_progress(Arc::clone(&counters.admission));
    for index in 0..TASKS {
        loop {
            counters.admission.begin(index);
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
    counters.admission.finish();
    Ok(())
}

#[test]
fn continuously_refilled_coalesced_inbox_is_fully_drained() {
    if isolate("continuously_refilled_coalesced_inbox_is_fully_drained") {
        return;
    }
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    shared.inboxes[0].signal.test_progress.enable();
    let scope = shared.begin_scope().unwrap();
    let counters = Arc::new(Counters::default());
    let outcome = thread::scope(|threads| {
        let stop = Stop(Arc::clone(&shared), Arc::clone(&counters));
        let worker_shared = Arc::clone(&shared);
        let worker = threads.spawn(move || super::run(worker_shared, CarrierId(0)));
        let startup_deadline = Instant::now() + WATCHDOG;
        while shared.inboxes[0].signal.waiting() == 0 && Instant::now() < startup_deadline {
            thread::yield_now();
        }
        if shared.inboxes[0].signal.waiting() == 0 {
            observe_refill_passive(&shared, &counters, "startup-deadline");
            wait_without_intervention(Duration::from_secs(1));
            observe_refill_passive(&shared, &counters, "startup-observation");
            let mut output = std::io::stdout().lock();
            writeln!(output, "refill-startup result=initial-wait-not-observed").unwrap();
            output.flush().unwrap();
            drop(output);
            let before = observe_refill_rich(&shared, scope, &counters);
            panic!("initial carrier wait was not observed: {before:?}");
        }
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
        let primary = match (&admission, &drained) {
            (Ok(Ok(())), Some(Ok(true))) => Primary::Passed,
            (Ok(Ok(())), _) => Primary::Drain,
            _ => Primary::Admission,
        };
        observe_refill_passive(&shared, &counters, "primary");
        let passive_later = primary != Primary::Passed;
        if passive_later {
            wait_without_intervention(Duration::from_secs(1));
            observe_refill_passive(&shared, &counters, "no-intervention");
        }
        let late_before_stop = admission.is_err().then(|| submitted_rx.try_recv());
        let mut output = std::io::stdout().lock();
        writeln!(output,
            "refill-primary result={primary:?} admission={admission:?} drained={drained:?} passive_later={passive_later} late_before_stop={late_before_stop:?}").unwrap();
        output.flush().unwrap();
        drop(output);
        let before = observe_refill_rich(&shared, scope, &counters);
        drop(stop);
        let producer = producer.join().map_err(crate::PanicReport::capture);
        let late_admission = admission.is_err().then(|| submitted_rx.try_recv());
        let worker = worker.join().map_err(crate::PanicReport::capture);
        writeln!(std::io::stdout().lock(),
            "refill-cleanup producer={producer:?} worker={worker:?} late_admission={late_admission:?}").unwrap();
        (primary, admission, drained, before, producer, worker)
    });
    let cleanup = shared.scope_report(scope);
    writeln!(std::io::stdout().lock(), "refill-final cleanup={cleanup:?}").unwrap();
    shared.finish_scope(scope);
    let (primary, admission, drained, before, producer, worker) = outcome;
    assert_eq!(primary, Primary::Passed, "primary refill deadline failed");
    admission
        .expect("continuous refill admission gate failed")
        .expect("admission returned a runtime error");
    assert!(
        matches!(drained, Some(Ok(true))),
        "accepted work did not drain: {drained:?}"
    );
    assert_complete(before, TASKS, &counters);
    producer.expect("producer panic retained after primary observation");
    worker.expect("carrier panic retained after primary observation");
    assert!(cleanup.failures.is_empty(), "{cleanup:?}");
    assert_eq!(
        (cleanup.aborted, cleanup.panicked, cleanup.failed_carriers),
        (0, 0, 0)
    );
    assert!(shared.inboxes[0].reclaimed.load(Ordering::Acquire));
}

#[test]
fn one_inbox_epoch_drains_multiple_receive_batches() {
    const BATCHED_TASKS: usize = 129;
    if isolate("one_inbox_epoch_drains_multiple_receive_batches") {
        return;
    }
    let shared = Arc::new(Shared::new(RuntimeConfig::default()));
    shared.inboxes[0].signal.test_progress.enable();
    let scope = shared.begin_scope().unwrap();
    let counters = Arc::new(Counters::default());
    let _progress = install_admission_progress(Arc::clone(&counters.admission));
    let empty_epoch = shared.inboxes[0].signal.version();
    for index in 0..BATCHED_TASKS {
        counters.admission.begin(index);
        let body = Arc::clone(&counters);
        shared
            .submit(scope, format!("batch-{index}"), move || {
                body.started.fetch_add(1, Ordering::SeqCst);
                body.returned.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        counters.accepted.fetch_add(1, Ordering::SeqCst);
    }
    counters.admission.finish();
    let queued_epoch = shared.inboxes[0].signal.version();
    assert_ne!(queued_epoch, empty_epoch);
    let (drained, before, final_epoch, worker) = thread::scope(|threads| {
        let stop = Stop(Arc::clone(&shared), Arc::clone(&counters));
        let carrier = Arc::clone(&shared);
        let worker = threads.spawn(move || super::run(carrier, CarrierId(0)));
        let drained = shared.wait_until(scope, None, Some(Instant::now() + WATCHDOG));
        observe_refill_passive(&shared, &counters, "multiple-batches");
        let passive_later = !matches!(&drained, Ok(true));
        if passive_later {
            wait_without_intervention(Duration::from_secs(1));
            observe_refill_passive(&shared, &counters, "multiple-batches-no-intervention");
        }
        let mut output = std::io::stdout().lock();
        writeln!(
            output,
            "refill-multiple-batches result={drained:?} passive_later={passive_later}"
        )
        .unwrap();
        output.flush().unwrap();
        drop(output);
        let before = observe_refill_rich(&shared, scope, &counters);
        let final_epoch = shared.inboxes[0].signal.version();
        drop(stop);
        (
            drained,
            before,
            final_epoch,
            worker.join().map_err(crate::PanicReport::capture),
        )
    });
    let cleanup = shared.scope_report(scope);
    shared.finish_scope(scope);
    assert!(
        matches!(drained, Ok(true)),
        "multiple batches did not drain: {drained:?}"
    );
    assert_eq!(final_epoch, queued_epoch, "backlog required another epoch");
    assert_complete(before, BATCHED_TASKS, &counters);
    worker.expect("carrier panic while draining multiple batches");
    assert!(cleanup.failures.is_empty(), "{cleanup:?}");
    assert!(shared.inboxes[0].reclaimed.load(Ordering::Acquire));
}
