use crate::{CarrierId, Runtime, control::Shared, signal::lock, support_test::run_isolated};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

fn isolate(name: &str) -> bool {
    const CHILD: &str = "VTHREAD_INGRESS_BOUNDARY_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        return false;
    }
    let exact = format!("carrier::carrier_ingress_test::{name}");
    let output = run_isolated(&exact, (CHILD, name), Duration::from_secs(25));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.timed_out && output.status.success() && stdout.contains("1 passed"),
        "isolated ingress failed: status={:?} timed_out={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
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

#[test]
fn accepted_ingress_progresses_while_its_notifier_is_paused() {
    if isolate("accepted_ingress_progresses_while_its_notifier_is_paused") {
        return;
    }
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
    let (mounted, mounted_rx) = mpsc::channel();
    let (resume, resume_rx) = mpsc::channel();
    shared
        .submit(scope, "initial epoch".into(), move || {
            mounted.send(thread::current().id()).unwrap();
            // Deliberately hold the real carrier after it handled the initial epoch.
            resume_rx.recv().unwrap();
        })
        .unwrap();

    let outcome = thread::scope(|threads| {
        // Locals release both gates and request stop before implicit scoped joins,
        // including observer panic or a failed bounded receive.
        let stop = Stop(Arc::clone(&shared));
        let carrier_shared = Arc::clone(&shared);
        let carrier = threads.spawn(move || super::run(carrier_shared, CarrierId(0)));
        let resume = Release(resume);
        let owner = mounted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let observed = shared.inboxes[0].signal.version();
        let (published, published_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        *lock(&shared.inboxes[0].before_notify_hook) = Some(Box::new(move || {
            published.send(()).unwrap();
            let _ = release_rx.recv();
        }));
        let release = Release(release);
        let (completed, completed_rx) = mpsc::channel();
        let producer_shared = Arc::clone(&shared);
        let producer = threads.spawn(move || {
            producer_shared.submit(scope, "visible before notification".into(), move || {
                let _ = completed.send(thread::current().id());
            })
        });
        published_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(shared.inboxes[0].signal.version(), observed);
        assert_eq!(shared.inboxes[0].pending(), 1);
        drop(resume);

        let ran = completed_rx.recv_timeout(Duration::from_secs(5));
        let pending = shared.inboxes[0].pending();
        let unchanged = shared.inboxes[0].signal.version() == observed;
        let mounts = shared.snapshot().stats.mounts();
        // Cleanup before asserting progress; a failing old-code run cannot strand
        // the publisher or carrier. The deadline is only a failure bound.
        drop(release);
        producer.join().unwrap().unwrap();
        drop(stop);
        carrier.join().unwrap();
        (ran, owner, pending, unchanged, mounts)
    });
    shared.finish_scope(scope);
    let (ran, owner, pending, unchanged, mounts) = outcome;
    assert!(
        unchanged,
        "producer must still be paused during observation"
    );
    assert_eq!(
        ran,
        Ok(owner),
        "visible ingress stalled: pending={pending}, mounts={mounts}"
    );
    assert_eq!(
        pending, 0,
        "visible ingress must drain before its epoch changes"
    );
}

#[test]
fn refill_between_idle_observation_and_wait_registration_is_not_lost() {
    if isolate("refill_between_idle_observation_and_wait_registration_is_not_lost") {
        return;
    }
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
    let (first, first_rx) = mpsc::channel();
    shared
        .submit(scope, "initial drain".into(), move || {
            first.send(()).unwrap()
        })
        .unwrap();
    let initial_epoch = shared.inboxes[0].signal.version();
    let (boundary, boundary_rx) = mpsc::channel();
    let (resume, resume_rx) = mpsc::channel();
    shared.inboxes[0].signal.before_wait(move || {
        boundary.send(()).unwrap();
        let _ = resume_rx.recv();
    });

    let outcome = thread::scope(|threads| {
        let stop = Stop(Arc::clone(&shared));
        let carrier_shared = Arc::clone(&shared);
        let carrier = threads.spawn(move || super::run(carrier_shared, CarrierId(0)));
        let resume = Release(resume);
        first_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        boundary_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(shared.inboxes[0].signal.waiting(), 0);
        let (second, second_rx) = mpsc::channel();
        shared
            .submit(scope, "boundary refill".into(), move || {
                second.send(()).unwrap()
            })
            .unwrap();
        let notified_epoch = shared.inboxes[0].signal.version();
        assert_ne!(notified_epoch, initial_epoch);
        assert_eq!(shared.inboxes[0].pending(), 1);
        drop(resume);
        let progress = second_rx.recv_timeout(Duration::from_secs(5));
        let pending = shared.inboxes[0].pending();
        let unchanged = shared.inboxes[0].signal.version() == notified_epoch;
        drop(stop);
        carrier.join().unwrap();
        (progress, pending, unchanged)
    });
    shared.finish_scope(scope);
    assert_eq!(outcome.0, Ok(()), "boundary refill did not run");
    assert_eq!(outcome.1, 0, "boundary refill remained queued");
    assert!(outcome.2, "boundary refill required another notification");
}

#[test]
fn an_idle_work_observation_remembers_remote_ingress_for_the_next_drive() {
    let shared = Arc::new(Shared::new(crate::RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let mut kernel = crate::kernel::Kernel::new(Arc::clone(&shared), CarrierId(0));
    assert!(!kernel.receive());
    shared.submit(scope, "visible".into(), || ()).unwrap();
    let observed = kernel.inbox.signal.version();
    kernel.wait_for_work(observed);
    let remembered = kernel.remote_pending();
    kernel.receive();
    while kernel.tick(false).unwrap() {}
    shared.finish_scope(scope);
    assert!(
        remembered,
        "idle predicate work must not depend on another signal"
    );
}

#[test]
fn wake_only_idle_work_does_not_claim_remote_starts() {
    let shared = Arc::new(Shared::new(crate::RuntimeConfig::default()));
    let scope = shared.begin_scope().unwrap();
    let (parker, waker) = crate::park_pair();
    shared
        .submit(scope, "wake only".into(), move || parker.park())
        .unwrap();
    let mut kernel = crate::kernel::Kernel::new(Arc::clone(&shared), CarrierId(0));
    assert!(!kernel.receive());
    assert!(kernel.tick(false).unwrap());
    waker.unpark();
    kernel.wait_for_work(kernel.inbox.signal.version());
    let claimed_starts = kernel.remote_pending();
    while kernel.tick(false).unwrap() {}
    shared.finish_scope(scope);
    assert!(!claimed_starts, "a wake notice is not an ingress packet");
}
