use std::{
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

use super::Signal;

#[test]
fn notification_before_wait_is_not_lost() {
    let signal = Signal::default();
    let observed = signal.version();
    signal.notify();
    signal.wait(observed, None);
    assert_ne!(signal.version(), observed);
}

#[test]
fn remote_notification_releases_a_waiter() {
    let signal = Arc::new(Signal::default());
    let observed = signal.version();
    let remote = Arc::clone(&signal);
    let thread = thread::spawn(move || remote.notify());
    signal.wait(observed, None);
    thread.join().expect("notifier");
    signal.wait(signal.version(), Some(Instant::now()));
}

#[test]
fn notification_releases_a_registered_waiter() {
    let signal = Arc::new(Signal::default());
    let observed = signal.version();
    let remote = Arc::clone(&signal);
    let deadline = Instant::now() + Duration::from_secs(5);
    let waiter = thread::spawn(move || {
        remote.wait(observed, Some(deadline));
        Instant::now() < deadline
    });

    while signal.waiters.load(Ordering::SeqCst) == 0 {
        thread::yield_now();
    }
    signal.notify();

    assert!(waiter.join().expect("waiter"));
}

#[test]
fn predicate_notification_releases_a_registered_waiter_without_changing_epoch() {
    let signal = Arc::new(Signal::default());
    let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = signal.version();
    let remote = Arc::clone(&signal);
    let remote_ready = Arc::clone(&ready);
    let deadline = Instant::now() + Duration::from_secs(5);
    let waiter = thread::spawn(move || {
        remote.wait_while(observed, Some(deadline), || {
            remote_ready.load(Ordering::Acquire)
        });
        Instant::now() < deadline
    });

    while signal.waiters.load(Ordering::SeqCst) == 0 {
        thread::yield_now();
    }
    ready.store(true, Ordering::Release);
    signal.notify_if_waiting();

    assert!(waiter.join().expect("waiter"));
    assert_eq!(signal.version(), observed);
}
