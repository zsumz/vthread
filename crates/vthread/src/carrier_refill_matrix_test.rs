//! Multi-producer and multi-carrier coverage around the remote receive window.

use crate::{
    CarrierId, Error, Runtime,
    control::Shared,
    signal::lock,
    support_test::{run_isolated, wait_without_intervention},
};
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const PRODUCERS: usize = 4;
const TASKS_PER_PRODUCER: usize = 129;
const TASKS: usize = PRODUCERS * TASKS_PER_PRODUCER;
const QUEUE_CAPACITY: usize = 65;
const WATCHDOG: Duration = Duration::from_secs(5);
const CHILD: &str = "VTHREAD_REFILL_MATRIX_CHILD";

#[derive(Default)]
struct Counts {
    accepted: AtomicUsize,
    returned: AtomicUsize,
    retries: AtomicUsize,
    executed: [AtomicUsize; 2],
}

struct Stop(Arc<Shared>);

impl Drop for Stop {
    fn drop(&mut self) {
        self.0.request_stop();
    }
}

fn isolate() -> bool {
    if std::env::var(CHILD).as_deref() == Ok("1") {
        return false;
    }
    let name = "carrier::carrier_refill_matrix_test::small_queues_progress_with_multiple_producers_and_carriers";
    let output = run_isolated(name, (CHILD, "1"), Duration::from_secs(25));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.timed_out && output.status.success() && stdout.contains("1 passed"),
        "isolated refill matrix failed: status={:?} timed_out={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status,
        output.timed_out,
    );
    true
}

fn record(shared: &Shared, counts: &Counts, phase: &str) {
    let pending = shared
        .inboxes
        .iter()
        .map(|inbox| inbox.pending())
        .collect::<Vec<_>>();
    let epochs = shared
        .inboxes
        .iter()
        .map(|inbox| inbox.signal.version())
        .collect::<Vec<_>>();
    let carriers = shared
        .inboxes
        .iter()
        .map(|inbox| inbox.signal.test_progress.snapshot())
        .collect::<Vec<_>>();
    let mut output = std::io::stdout().lock();
    writeln!(
        output,
        "refill-matrix phase={phase} accepted={} returned={} retries={} \
         pending={pending:?} epochs={epochs:?} carriers={carriers:?}",
        counts.accepted.load(Ordering::Acquire),
        counts.returned.load(Ordering::Acquire),
        counts.retries.load(Ordering::Acquire),
    )
    .unwrap();
    output.flush().unwrap();
}

#[test]
fn small_queues_progress_with_multiple_producers_and_carriers() {
    if isolate() {
        return;
    }
    let config = Runtime::builder()
        .carriers(2)
        .max_vthreads(TASKS)
        .carrier_queue_capacity(QUEUE_CAPACITY)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    for inbox in &shared.inboxes {
        inbox.signal.test_progress.enable();
    }
    let scope = shared.begin_scope().unwrap();
    let counts = Arc::new(Counts::default());
    let outcome = thread::scope(|threads| {
        let stop = Stop(Arc::clone(&shared));
        let carriers = (0..2)
            .map(|index| {
                let carrier = Arc::clone(&shared);
                threads.spawn(move || super::run(carrier, CarrierId(index)))
            })
            .collect::<Vec<_>>();
        let wait_deadline = Instant::now() + WATCHDOG;
        while shared
            .inboxes
            .iter()
            .map(|inbox| inbox.signal.waiting())
            .sum::<usize>()
            != 2
            && Instant::now() < wait_deadline
        {
            thread::yield_now();
        }
        assert!(
            shared
                .inboxes
                .iter()
                .all(|inbox| inbox.signal.waiting() == 1),
            "both carriers must register their initial wait"
        );
        let (hooked, hooked_rx) = mpsc::channel();
        let releases = shared
            .inboxes
            .iter()
            .enumerate()
            .map(|(index, inbox)| {
                let hooked = hooked.clone();
                let (release, release_rx) = mpsc::channel();
                *lock(&inbox.before_notify_hook) = Some(Box::new(move || {
                    let _ = hooked.send(index);
                    let _ = release_rx.recv();
                }));
                release
            })
            .collect::<Vec<_>>();
        drop(hooked);
        let (done, done_rx) = mpsc::channel();
        let producers = (0..PRODUCERS)
            .map(|producer| {
                let shared = Arc::clone(&shared);
                let counts = Arc::clone(&counts);
                let done = done.clone();
                threads.spawn(move || {
                    let result = fill(&shared, scope, producer, &counts);
                    let _ = done.send(result);
                })
            })
            .collect::<Vec<_>>();
        drop(done);
        let hook_deadline = Instant::now() + WATCHDOG;
        let mut hook_order = Vec::new();
        for _ in 0..2 {
            if let Ok(index) =
                hooked_rx.recv_timeout(hook_deadline.saturating_duration_since(Instant::now()))
            {
                hook_order.push(index);
            }
        }
        hook_order.sort_unstable();
        if hook_order == [0, 1] {
            let fill_deadline = Instant::now() + WATCHDOG;
            while (shared
                .inboxes
                .iter()
                .any(|inbox| inbox.pending() != QUEUE_CAPACITY)
                || counts.retries.load(Ordering::Acquire) == 0)
                && Instant::now() < fill_deadline
            {
                thread::yield_now();
            }
        }
        let queues_full = shared
            .inboxes
            .iter()
            .all(|inbox| inbox.pending() == QUEUE_CAPACITY);
        let retried = counts.retries.load(Ordering::Acquire) != 0;
        record(&shared, &counts, "before-notification");
        for release in releases {
            let _ = release.send(());
        }
        assert_eq!(hook_order, [0, 1], "both notifier hooks must pause");
        assert!(
            queues_full,
            "both queues must fill while notification is paused"
        );
        assert!(retried, "full queues must reject at least one admission");
        let deadline = Instant::now() + WATCHDOG;
        let admission: Result<(), String> = (0..PRODUCERS).try_for_each(|_| {
            done_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|error| format!("{error:?}"))?
                .map_err(|error| format!("{error:?}"))
        });
        let drained = admission
            .is_ok()
            .then(|| shared.wait_until(scope, None, Some(Instant::now() + WATCHDOG)));
        let passed = admission.is_ok() && matches!(drained, Some(Ok(true)));
        record(&shared, &counts, "primary");
        if !passed {
            wait_without_intervention(Duration::from_secs(1));
            record(&shared, &counts, "no-intervention");
        }
        let report = shared.scope_report(scope);
        let snapshot = shared.snapshot();
        let queued = shared
            .inboxes
            .iter()
            .map(|inbox| inbox.pending())
            .sum::<usize>();
        writeln!(
            std::io::stdout().lock(),
            "refill-matrix-rich admission={admission:?} drained={drained:?} \
             report={report:?} snapshot={snapshot:?}"
        )
        .unwrap();
        drop(stop);
        for producer in producers {
            producer.join().unwrap();
        }
        for carrier in carriers {
            carrier.join().unwrap();
        }
        (admission, drained, report, snapshot, queued)
    });
    shared.finish_scope(scope);
    assert!(
        outcome.0.is_ok(),
        "matrix admission failed: {:?}",
        outcome.0
    );
    assert!(matches!(outcome.1, Some(Ok(true))), "matrix did not drain");
    assert_eq!(counts.accepted.load(Ordering::Acquire), TASKS);
    assert_eq!(counts.returned.load(Ordering::Acquire), TASKS);
    assert!(
        counts
            .executed
            .iter()
            .all(|count| count.load(Ordering::Acquire) != 0)
    );
    assert_eq!(
        (outcome.2.completed, outcome.3.active, outcome.4),
        (TASKS as u64, 0, 0)
    );
    assert!(outcome.2.failures.is_empty(), "{:?}", outcome.2);
    assert!(
        shared
            .inboxes
            .iter()
            .all(|inbox| inbox.reclaimed.load(Ordering::Acquire))
    );
}

fn fill(shared: &Shared, scope: u64, producer: usize, counts: &Arc<Counts>) -> crate::Result<()> {
    for index in 0..TASKS_PER_PRODUCER {
        loop {
            let body = Arc::clone(counts);
            match shared.submit(scope, format!("producer-{producer}-{index}"), move || {
                let carrier = crate::worker_context::current_carrier().unwrap();
                body.executed[carrier.0].fetch_add(1, Ordering::Release);
                body.returned.fetch_add(1, Ordering::Release);
            }) {
                Ok(_) => {
                    counts.accepted.fetch_add(1, Ordering::Release);
                    break;
                }
                Err(Error::Capacity {
                    resource: crate::error::CapacityResource::CarrierQueue,
                    ..
                }) => {
                    counts.retries.fetch_add(1, Ordering::Relaxed);
                    thread::yield_now();
                }
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}
