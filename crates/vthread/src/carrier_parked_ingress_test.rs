//! Parked carriers retain a coalesced notification handoff across publishers.

use crate::{CarrierId, Runtime, control::Shared, signal::lock, support_test::run_isolated};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

const WATCHDOG: Duration = Duration::from_secs(5);
const CHILD: &str = "VTHREAD_PARKED_INGRESS_CHILD";

#[derive(Clone, Copy)]
enum Boundary {
    Parked,
    Registering,
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

fn isolate(name: &str) -> bool {
    if std::env::var(CHILD).as_deref() == Ok(name) {
        return false;
    }
    let exact = format!("carrier::carrier_parked_ingress_test::{name}");
    let output = run_isolated(&exact, (CHILD, name), WATCHDOG * 5);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.timed_out && output.status.success() && stdout.contains("1 passed"),
        "isolated parked ingress failed: status={:?} timed_out={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status,
        output.timed_out,
    );
    true
}

fn exercise(boundary: Boundary) {
    let config = Runtime::builder()
        .carriers(1)
        .max_vthreads(2)
        .carrier_queue_capacity(2)
        .stack_cache_capacity(2)
        .build()
        .unwrap()
        .config();
    let shared = Arc::new(Shared::new(config));
    let scope = shared.begin_scope().unwrap();
    let registration = matches!(boundary, Boundary::Registering).then(|| {
        let (reached, reached_rx) = mpsc::channel();
        let (resume, resume_rx) = mpsc::channel();
        shared.inboxes[0].signal.before_wait(move || {
            reached.send(()).unwrap();
            let _ = resume_rx.recv();
        });
        (reached_rx, Release(resume))
    });

    let outcome = thread::scope(|threads| {
        let stop = Stop(Arc::clone(&shared));
        let carrier_shared = Arc::clone(&shared);
        let carrier = threads.spawn(move || super::run(carrier_shared, CarrierId(0)));
        let mut registration = registration;
        if let Some((reached, _)) = &registration {
            reached.recv_timeout(WATCHDOG).unwrap();
            assert_eq!(shared.inboxes[0].signal.waiting(), 0);
        } else {
            let deadline = Instant::now() + WATCHDOG;
            while shared.inboxes[0].signal.waiting() == 0 && Instant::now() < deadline {
                thread::yield_now();
            }
            assert_eq!(shared.inboxes[0].signal.waiting(), 1);
        }

        let observed = shared.inboxes[0].signal.version();
        let (published, published_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        *lock(&shared.inboxes[0].before_notify_hook) = Some(Box::new(move || {
            published.send(()).unwrap();
            let _ = release_rx.recv();
        }));
        let release = Release(release);
        let first_shared = Arc::clone(&shared);
        let first = threads.spawn(move || {
            first_shared
                .submit(scope, "first paused notifier".into(), || ())
                .map(|_| ())
        });
        published_rx.recv_timeout(WATCHDOG).unwrap();
        assert_eq!(shared.inboxes[0].pending(), 1);

        let (ran, ran_rx) = mpsc::channel();
        let (submitted, submitted_rx) = mpsc::channel();
        let second_shared = Arc::clone(&shared);
        let second = threads.spawn(move || {
            let result = second_shared
                .submit(scope, "later publisher".into(), move || {
                    let _ = ran.send(());
                })
                .map(|_| ());
            let _ = submitted.send(result.is_ok());
            result
        });
        let submitted = submitted_rx.recv_timeout(WATCHDOG);
        if let Some((_, registration_release)) = registration.take() {
            drop(registration_release);
        }
        let ran = ran_rx.recv_timeout(WATCHDOG);
        let pending = shared.inboxes[0].pending();
        let waiting = shared.inboxes[0].signal.waiting();
        let unchanged_epoch = shared.inboxes[0].signal.version() == observed;

        drop(release);
        let first = first.join().unwrap();
        let second = second.join().unwrap();
        drop(stop);
        carrier.join().unwrap();
        (
            submitted,
            ran,
            pending,
            waiting,
            unchanged_epoch,
            first,
            second,
        )
    });
    shared.finish_scope(scope);

    assert_eq!(outcome.0, Ok(true), "later submission did not return");
    assert_eq!(
        outcome.1,
        Ok(()),
        "later task stalled before the first notifier resumed: submitted={:?} pending={} waiting={} unchanged_epoch={}",
        outcome.0,
        outcome.2,
        outcome.3,
        outcome.4,
    );
    assert_eq!(outcome.2, 0, "published tasks remained queued");
    assert!(outcome.4, "coalesced handoff advanced the signal epoch");
    outcome.5.unwrap();
    outcome.6.unwrap();
}

#[test]
fn a_later_publisher_wakes_an_already_parked_carrier() {
    if !isolate("a_later_publisher_wakes_an_already_parked_carrier") {
        exercise(Boundary::Parked);
    }
}

#[test]
fn registration_rechecks_the_queue_before_sleeping() {
    if !isolate("registration_rechecks_the_queue_before_sleeping") {
        exercise(Boundary::Registering);
    }
}
