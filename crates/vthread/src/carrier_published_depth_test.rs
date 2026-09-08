//! Published inbox depth remains actionable before its coalesced notification.

use crate::{CarrierId, Runtime, control::Shared, signal::lock, support_test::run_isolated};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const CHILD: &str = "VTHREAD_PUBLISHED_DEPTH_CHILD";
const TEST: &str =
    "carrier::carrier_published_depth_test::published_depth_runs_during_sustained_ready_work";

fn isolate() -> bool {
    if std::env::var(CHILD).as_deref() == Ok("1") {
        return false;
    }
    let output = run_isolated(TEST, (CHILD, "1"), Duration::from_secs(25));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.timed_out && output.status.success() && stdout.contains("1 passed"),
        "isolated published-depth test failed: status={:?} timed_out={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status,
        output.timed_out,
    );
    true
}

struct Release(mpsc::Sender<()>);

impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

struct Stop(Arc<Shared>);

impl Drop for Stop {
    fn drop(&mut self) {
        self.0.request_stop();
    }
}

struct ReleaseYield(Arc<AtomicBool>);

impl Drop for ReleaseYield {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn published_depth_runs_during_sustained_ready_work() {
    if isolate() {
        return;
    }
    let config = Runtime::builder()
        .carriers(1)
        .max_vthreads(3)
        .carrier_queue_capacity(3)
        .stack_cache_capacity(3)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    let (mounted, mounted_rx) = mpsc::channel();
    let (resume, resume_rx) = mpsc::channel();
    shared
        .submit(scope, "block first dispatch".into(), move || {
            mounted.send(()).unwrap();
            resume_rx.recv().unwrap();
        })
        .unwrap();
    let release_yield = Arc::new(AtomicBool::new(false));
    let yielding = Arc::clone(&release_yield);
    shared
        .submit(scope, "sustain ready work".into(), move || {
            while !yielding.load(Ordering::Acquire) {
                if crate::yield_now().is_err() {
                    break;
                }
            }
        })
        .unwrap();

    let outcome = thread::scope(|threads| {
        let stop = Stop(Arc::clone(&shared));
        let release_yield = ReleaseYield(release_yield);
        let carrier_shared = Arc::clone(&shared);
        let carrier = threads.spawn(move || super::run(carrier_shared, CarrierId(0)));
        let resume = Release(resume);
        mounted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(shared.inboxes[0].pending(), 0);
        let epoch = shared.inboxes[0].signal.version();

        let (published, published_rx) = mpsc::channel();
        let (notify, notify_rx) = mpsc::channel();
        *lock(&shared.inboxes[0].before_notify_hook) = Some(Box::new(move || {
            published.send(()).unwrap();
            let _ = notify_rx.recv();
        }));
        let notify = Release(notify);
        let (third_ran, third_ran_rx) = mpsc::channel();
        let producer_shared = Arc::clone(&shared);
        let producer = threads.spawn(move || {
            producer_shared.submit(scope, "published before notify".into(), move || {
                third_ran.send(()).unwrap();
            })
        });
        published_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(shared.inboxes[0].signal.version(), epoch);
        assert_eq!(shared.inboxes[0].pending(), 1);

        drop(resume);
        let late = third_ran_rx.recv_timeout(Duration::from_secs(5));
        let pending = shared.inboxes[0].pending();
        let unchanged = shared.inboxes[0].signal.version() == epoch;
        drop(release_yield);
        drop(notify);
        producer.join().unwrap().unwrap();
        let drained = shared.wait_until(scope, None, Some(Instant::now() + Duration::from_secs(5)));
        drop(stop);
        carrier.join().unwrap();
        (late, pending, unchanged, drained)
    });
    shared.finish_scope(scope);
    assert_eq!(
        outcome.0,
        Ok(()),
        "published ingress stalled during sustained ready work: pending={}, unchanged_epoch={}",
        outcome.1,
        outcome.2,
    );
    assert_eq!(outcome.1, 0, "published task remained queued");
    assert!(outcome.2, "the producer notification must still be paused");
    assert!(matches!(outcome.3, Ok(true)), "scope did not drain");
}
