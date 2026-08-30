use std::{sync::Arc, thread, time::Instant};

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
