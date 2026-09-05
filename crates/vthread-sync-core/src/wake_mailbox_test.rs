use crate::wake_atomic::{Arc, AtomicU64, Ordering, model, thread};

use super::WakeMailbox;

#[test]
fn owner_acknowledges_without_clearing_the_publication_word() {
    model(|| {
        let mailbox = WakeMailbox::default();
        assert!(!mailbox.has_pending());
        assert!(!mailbox.publish(62));
        assert!(mailbox.has_pending());
        assert_eq!(mailbox.pop(), Some(62));
        assert_eq!(mailbox.published.0.load(Ordering::Relaxed), 1 << 62);
        assert!(!mailbox.has_pending());
        assert!(!mailbox.publish(62));
        assert_eq!(mailbox.published.0.load(Ordering::Relaxed), 0);
        assert_eq!(mailbox.pop(), Some(62));
        assert_eq!(mailbox.pop(), None);
    });
}

#[test]
fn captured_batch_prevents_route_reuse_from_starving_older_work() {
    model(|| {
        let mailbox = WakeMailbox::new();
        for route in [62, 1, 0] {
            assert!(!mailbox.publish(route));
        }
        assert_eq!(mailbox.pop(), Some(0));
        assert!(!mailbox.publish(0));
        assert!(mailbox.has_pending());
        assert!(mailbox.arm_wait());
        assert_eq!(mailbox.pop(), Some(1));
        assert_eq!(mailbox.pop(), Some(62));
        assert_eq!(mailbox.pop(), Some(0));
        assert!(!mailbox.has_pending());
        assert_eq!(mailbox.pop(), None);
    });
}

#[test]
fn racing_producers_publish_payloads_exactly_once() {
    model(|| {
        let mailbox = Arc::new(WakeMailbox::new());
        let payloads = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
        let writers = (0..2)
            .map(|route| {
                let mailbox = Arc::clone(&mailbox);
                let payloads = Arc::clone(&payloads);
                thread::spawn(move || {
                    payloads[route].store(route as u64 + 1, Ordering::Relaxed);
                    assert!(!mailbox.publish(route));
                })
            })
            .collect::<Vec<_>>();
        let mut seen = [false; 2];
        let mut observe = |route: usize| {
            assert!(!seen[route], "route became runnable twice");
            assert_eq!(payloads[route].load(Ordering::Relaxed), route as u64 + 1);
            seen[route] = true;
        };
        // Observe before joining: a join must not mask a missing publication Acquire.
        for _ in 0..2 {
            if let Some(route) = mailbox.pop() {
                observe(route);
            }
        }
        for writer in writers {
            writer.join().unwrap();
        }
        while let Some(route) = mailbox.pop() {
            observe(route);
        }
        assert_eq!(seen, [true; 2]);
        assert!(!mailbox.has_pending());
    });
}

#[test]
fn arming_race_always_observes_work_or_requests_notification() {
    model(|| {
        let mailbox = Arc::new(WakeMailbox::new());
        let payload = Arc::new(AtomicU64::new(0));
        let writer = {
            let mailbox = Arc::clone(&mailbox);
            let payload = Arc::clone(&payload);
            thread::spawn(move || {
                payload.store(42, Ordering::Relaxed);
                mailbox.publish(0)
            })
        };
        let ready = mailbox.arm_wait();
        let notified = writer.join().unwrap();
        assert!(ready || notified, "work was lost across the sleep boundary");
        mailbox.disarm_wait();
        assert_eq!(mailbox.pop(), Some(0));
        assert_eq!(payload.load(Ordering::Relaxed), 42);
        assert_eq!(mailbox.pop(), None);
    });
}

#[test]
fn disarming_does_not_erase_a_racing_publication() {
    model(|| {
        let mailbox = Arc::new(WakeMailbox::new());
        assert!(!mailbox.arm_wait());
        let writer = {
            let mailbox = Arc::clone(&mailbox);
            thread::spawn(move || mailbox.publish(0))
        };
        mailbox.disarm_wait();
        writer.join().unwrap();
        assert_eq!(mailbox.pop(), Some(0));
        assert_eq!(mailbox.pop(), None);
        assert!(!mailbox.publish(0));
        assert_eq!(mailbox.pop(), Some(0));
    });
}

#[test]
fn route_reservation_protects_payload_until_owner_acknowledges_and_copies() {
    model(|| {
        let mailbox = Arc::new(WakeMailbox::new());
        let payload = Arc::new(AtomicU64::new(11));
        let reserved = Arc::new(AtomicU64::new(1));
        assert!(!mailbox.publish(0));
        let writer = {
            let mailbox = Arc::clone(&mailbox);
            let payload = Arc::clone(&payload);
            let reserved = Arc::clone(&reserved);
            thread::spawn(move || {
                // The wait winner is already unique; this mirrors the route reuse gate.
                if reserved.load(Ordering::Acquire) != 0 {
                    return false;
                }
                reserved.store(1, Ordering::Relaxed);
                payload.store(27, Ordering::Relaxed);
                assert!(!mailbox.publish(0));
                true
            })
        };
        assert_eq!(mailbox.pop(), Some(0));
        assert_eq!(payload.load(Ordering::Relaxed), 11);
        reserved.store(0, Ordering::Release);
        let republished = writer.join().unwrap();
        assert_eq!(mailbox.has_pending(), republished);
        if republished {
            assert_eq!(mailbox.pop(), Some(0));
            assert_eq!(payload.load(Ordering::Relaxed), 27);
            reserved.store(0, Ordering::Release);
        }
        assert_eq!(mailbox.pop(), None);
        assert_eq!(reserved.load(Ordering::Acquire), 0);
    });
}

#[test]
fn sleep_arming_preserves_acknowledged_parity_across_reuse() {
    model(|| {
        let mailbox = WakeMailbox::new();
        assert!(!mailbox.publish(0));
        assert_eq!(mailbox.pop(), Some(0));
        assert!(!mailbox.arm_wait());
        assert!(!mailbox.arm_wait());
        assert!(mailbox.publish(0));
        assert!(mailbox.arm_wait());
        mailbox.disarm_wait();
        assert_eq!(mailbox.pop(), Some(0));
        assert!(!mailbox.has_pending());
    });
}

#[test]
#[should_panic(expected = "mailbox route out of bounds")]
fn sleeping_bit_cannot_be_used_as_a_route() {
    model(|| {
        WakeMailbox::new().publish(WakeMailbox::ROUTES);
    });
}
