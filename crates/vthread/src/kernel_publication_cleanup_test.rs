//! Held claims retain owned stacks and borrowed ancestors without blocking peers.

use super::kernel_publication_test::{observe_progress, shared, unrelated};
use crate::{
    CarrierId, TaskFailure,
    kernel::Kernel,
    wait::wait_publication_probe_test::{PausedPublication, Stage},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};

struct DropCount(Arc<AtomicUsize>);

impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn scope_abort_retains_a_held_owned_wait() {
    scope_abort(false, false);
}

#[test]
fn scope_abort_retains_a_held_borrowed_wait_and_ancestor() {
    scope_abort(true, false);
}

#[test]
fn dispatch_abort_retains_a_ready_ancestor_of_a_held_borrowed_wait() {
    scope_abort(true, true);
}

fn scope_abort(borrowed: bool, during_dispatch: bool) {
    let shared = shared();
    let scope = shared.begin_scope().unwrap();
    let other = shared
        .begin_owned(crate::ScopeOptions::default(), true)
        .unwrap();
    let (parker, waker) = crate::park_pair();
    let cell = parker.wait.clone();
    let drops = Arc::new(AtomicUsize::new(0));
    let lifetime = DropCount(Arc::clone(&drops));
    shared
        .submit(scope, "retained ancestor".into(), move || {
            let lifetime = lifetime;
            let parked = || {
                let _borrow = &lifetime;
                parker.park().unwrap();
            };
            if borrowed {
                crate::local_scope(|local| -> crate::Result<()> {
                    let _child = local.spawn("borrowed recipient", parked)?;
                    loop {
                        crate::yield_now()?;
                    }
                })
                .unwrap();
            } else {
                parked();
            }
            drop(lifetime);
        })
        .unwrap();
    let progress = unrelated(&shared, other);
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    assert!(kernel.tick(false).unwrap());
    kernel.receive_local();
    assert!(kernel.tick(false).unwrap());
    if borrowed {
        assert!(kernel.tick(false).unwrap());
        assert!(kernel.tick(false).unwrap());
    }
    assert_eq!(progress.load(Ordering::Relaxed), 1);
    assert_eq!(kernel.stats.parks, 1);
    thread::scope(|threads| {
        let pause = PausedPublication::install(&cell);
        let publisher = threads.spawn(|| waker.unpark());
        pause.observe(Stage::NoticePublished);
        let (returned, resumed) = mpsc::channel();
        let observer = threads.spawn(move || observe_progress(pause, resumed));
        if during_dispatch {
            // Force the already-ready ancestor to be selected before the peer.
            let peer = kernel.ready.pop_front().unwrap();
            assert_eq!(kernel.task(peer).execution().scope(), other);
            kernel.ready.push_back(peer);
            shared.abort_scope(scope, TaskFailure::ScopeStalled);
            assert!(kernel.tick(true).unwrap());
        } else {
            kernel.abort(Some(scope), TaskFailure::ScopeStalled);
        }
        assert_eq!(
            drops.load(Ordering::Relaxed),
            0,
            "ancestor reclaimed before publication"
        );
        assert!(cell.publication_is_held());
        assert!(kernel.tick(false).unwrap());
        assert_eq!(progress.load(Ordering::Relaxed), 2);
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        assert!(cell.publication_is_held());
        returned.send(()).unwrap();
        observer.join().unwrap();
        publisher.join().unwrap();
    });
    while kernel.tick(true).unwrap() {}
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert_eq!(shared.snapshot().active, 0);
    shared.finish_scope(scope);
    shared.finish_scope(other);
}

#[test]
fn registration_error_retains_a_held_generation_before_cleanup() {
    failed_registration(false);
}

#[test]
fn registration_panic_retains_a_held_generation_before_cleanup() {
    failed_registration(true);
}

fn failed_registration(panic: bool) {
    let shared = shared();
    let scope = shared.begin_scope().unwrap();
    let progress = unrelated(&shared, scope);
    let (parker, _waker) = crate::park_pair();
    let cell = parker.wait.clone();
    let drops = Arc::new(AtomicUsize::new(0));
    let lifetime = DropCount(Arc::clone(&drops));
    let (attached, attachment) = mpsc::channel();
    let (fail, failure) = mpsc::channel();
    shared
        .submit(scope, "failed registration".into(), move || {
            let _lifetime = lifetime;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                parker.park_registered(|token, registration| -> crate::Result<()> {
                    attached.send((token, registration.clone())).unwrap();
                    failure.recv().unwrap();
                    if panic {
                        std::panic::panic_any("original registration panic");
                    }
                    Err(crate::Error::WouldBlock)
                })
            }));
            if panic {
                assert_eq!(
                    *result.unwrap_err().downcast::<&str>().unwrap(),
                    "original registration panic"
                );
            } else {
                assert!(matches!(result.unwrap(), Err(crate::Error::WouldBlock)));
            }
        })
        .unwrap();
    let mut kernel = Kernel::new(Arc::clone(&shared), CarrierId(0));
    kernel.receive();
    assert!(kernel.tick(false).unwrap());
    assert_eq!(progress.load(Ordering::Relaxed), 1);
    thread::scope(|threads| {
        let pause = PausedPublication::install(&cell);
        let publisher = threads.spawn(move || {
            let (token, registration) = attachment.recv().unwrap();
            assert!(registration.select_ready(token));
        });
        let (returned, resumed) = mpsc::channel();
        let observer = threads.spawn(move || {
            pause.observe(Stage::NoticePublished);
            fail.send(()).unwrap();
            observe_progress(pause, resumed)
        });
        assert!(kernel.tick(false).unwrap());
        assert_eq!(
            drops.load(Ordering::Relaxed),
            0,
            "error escaped before safe retirement"
        );
        assert!(cell.publication_is_held());
        assert_eq!(kernel.stats.parks, 1);
        assert!(kernel.tick(false).unwrap());
        assert_eq!(progress.load(Ordering::Relaxed), 2);
        assert!(cell.publication_is_held());
        returned.send(()).unwrap();
        observer.join().unwrap();
        publisher.join().unwrap();
    });
    while kernel.tick(true).unwrap() {}
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert_eq!(shared.snapshot().active, 0);
    shared.finish_scope(scope);
}
