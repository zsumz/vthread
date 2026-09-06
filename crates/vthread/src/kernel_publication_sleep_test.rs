//! Real carrier sleep, completion notification and selected-owner abandonment.

use super::kernel_publication_test::shared;
use crate::{
    Error, TaskFailure,
    wait::wait_publication_probe_test::{PausedPublication, Stage},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

struct Stop(Arc<crate::control::Shared>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.request_stop();
    }
}

struct Completed(Arc<AtomicUsize>);
impl Drop for Completed {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    Resume,
    Abandon,
    Stop,
    Fault,
}

#[test]
fn a_sleeping_park_owner_discovers_deferred_completion() {
    sleeping_owner(false, End::Resume);
}

#[test]
fn a_sleeping_mutex_owner_discovers_deferred_completion() {
    sleeping_owner(true, End::Resume);
}

#[test]
fn a_sleeping_park_owner_retires_selected_abandonment() {
    sleeping_owner(false, End::Abandon);
}

#[test]
fn a_sleeping_mutex_owner_recovers_selected_ownership_on_abandonment() {
    sleeping_owner(true, End::Abandon);
}

#[test]
fn global_stop_retains_a_held_park_publication() {
    sleeping_owner(false, End::Stop);
}

#[test]
fn global_stop_recovers_held_mutex_ownership() {
    sleeping_owner(true, End::Stop);
}

#[test]
fn a_failed_carrier_retires_a_held_park_publication() {
    sleeping_owner(false, End::Fault);
}

#[test]
fn a_failed_carrier_recovers_held_mutex_ownership() {
    sleeping_owner(true, End::Fault);
}

fn sleeping_owner(resource: bool, end: End) {
    let shared = shared();
    let scope = shared.begin_scope().unwrap();
    let other = shared
        .begin_owned(crate::ScopeOptions::default(), true)
        .unwrap();
    let mutex = Arc::new(crate::sync::Mutex::with_wait_capacity(0_usize, 1).unwrap());
    let (parker, waker) = crate::park_pair();
    let (attached, attachment) = mpsc::channel();
    let (resumed, resumption) = mpsc::channel();
    let completed = Arc::new(AtomicUsize::new(0));
    let lifetime = Completed(Arc::clone(&completed));
    let waiting = Arc::clone(&mutex);
    shared
        .submit(scope, "sleeping recipient".into(), move || {
            let _lifetime = lifetime;
            let owner = thread::current().id();
            if resource {
                let mounted = crate::context::current().unwrap();
                attached
                    .send(
                        mounted
                            .execution()
                            .unwrap()
                            .synchronization_wait()
                            .unwrap()
                            .clone(),
                    )
                    .unwrap();
                *waiting.lock().unwrap() += 1;
            } else {
                attached.send(parker.wait.clone()).unwrap();
                assert_eq!(parker.park().unwrap(), crate::ParkOutcome::Ready);
            }
            assert_eq!(thread::current().id(), owner);
            resumed.send(()).unwrap();
        })
        .unwrap();
    thread::scope(|threads| {
        // Stop is inside this lexical closure, so even an assertion failure stops
        // the carrier before scoped-thread joining. Pause Drop releases first.
        let _stop = Stop(Arc::clone(&shared));
        let (held, holding) = mpsc::channel();
        let (unlock, release) = mpsc::channel();
        let publishing = Arc::clone(&mutex);
        let publisher = threads.spawn(move || {
            let guard = resource.then(|| publishing.try_lock().unwrap());
            held.send(()).unwrap();
            let _ = release.recv();
            if resource {
                drop(guard);
            } else {
                waker.unpark();
            }
        });
        holding.recv_timeout(Duration::from_secs(5)).unwrap();
        let carrier_shared = Arc::clone(&shared);
        let carrier =
            threads.spawn(move || crate::carrier::run(carrier_shared, crate::CarrierId(0)));
        let cell = attachment.recv_timeout(Duration::from_secs(5)).unwrap();
        crate::support_test::until(|| shared.inboxes[0].signal.waiting() == 1);
        let mut pause = PausedPublication::install(&cell);
        unlock.send(()).unwrap();
        let (published, deferred) = pause.observe_routed_pair();
        assert_ne!(published.thread, deferred.thread);
        assert_ne!(deferred.thread, thread::current().id());
        if end == End::Abandon {
            shared.abort_scope(scope, TaskFailure::ScopeStalled);
        }
        if end == End::Fault {
            shared.fail_after_resume.store(true, Ordering::Release);
        }
        let (progressed, progress) = mpsc::channel();
        shared
            .submit(other, "unrelated arrival".into(), move || {
                progressed.send(thread::current().id()).unwrap();
            })
            .unwrap();
        assert_eq!(
            progress.recv_timeout(Duration::from_secs(5)).unwrap(),
            deferred.thread
        );
        assert_eq!(completed.load(Ordering::Relaxed), 0);
        assert!(cell.publication_is_held());
        if resource {
            assert_eq!(mutex.waiting(), 1);
            assert!(matches!(mutex.try_lock(), Err(Error::WouldBlock)));
        }
        if end == End::Stop {
            shared.request_stop();
        }
        if end != End::Resume {
            pause.observe(Stage::RetirementDeferred);
            assert!(!shared.inboxes[0].scheduler_stopped.load(Ordering::Acquire));
            assert_eq!(completed.load(Ordering::Relaxed), 0);
            assert!(cell.publication_is_held());
        }
        // There is no next request to wake this owner. Only completing the held
        // publication can make it resume/retire after it actually arms sleep.
        crate::support_test::until(|| shared.inboxes[0].signal.waiting() == 1);
        pause.release();
        publisher.join().unwrap();
        if end == End::Resume {
            resumption.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        crate::support_test::until(|| {
            completed.load(Ordering::Relaxed) == 1 && shared.snapshot().active == 0
        });
        if resource {
            assert_eq!(mutex.waiting(), 0);
            assert_eq!(*mutex.try_lock().unwrap(), usize::from(end == End::Resume));
        }
        assert_eq!(
            shared.scope_report(scope).aborted,
            u64::from(end != End::Resume)
        );
        assert_eq!(
            shared.scope_report(scope).failures.entries().len(),
            usize::from(end == End::Fault)
        );
        shared.request_stop();
        carrier.join().unwrap();
        assert!(shared.inboxes[0].reclaimed.load(Ordering::Acquire));
    });
    shared.finish_scope(scope);
    shared.finish_scope(other);
}
