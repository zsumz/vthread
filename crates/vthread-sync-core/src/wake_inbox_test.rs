use crate::wake_atomic::{Arc, model, thread};

use super::{WakeInbox, WakePacket};

fn packet(route: usize, generation: u64) -> WakePacket {
    WakePacket {
        route,
        task: generation * 7,
        wait: generation * 11,
        selection: generation * 13,
    }
}

#[test]
fn payload_slots_and_routing_cursors_retain_their_cacheline_layout() {
    model(|| {
        let inbox = WakeInbox::new(1);
        assert_eq!(std::mem::size_of::<super::Slot>(), 32);
        assert_eq!(std::mem::size_of::<super::Head>(), 64);
        assert_eq!(std::mem::size_of::<super::Consumer>(), 64);
        let head = std::ptr::from_ref(&inbox.head) as usize;
        let owner = std::ptr::from_ref(&inbox.consumer) as usize;
        let mailbox = std::ptr::from_ref(&inbox.mailbox) as usize;
        assert!(head.abs_diff(owner) >= 64);
        assert!(head.abs_diff(mailbox) >= 64);
        assert!(owner.abs_diff(mailbox) >= 64);
    });
}

#[test]
fn both_lanes_retain_older_batches_and_alternate_under_republication() {
    model(|| {
        let inbox = WakeInbox::new(66);
        for route in [0, 2, 65, 63] {
            inbox.push(packet(route, 1), || {}).unwrap();
        }
        assert_eq!(inbox.pop(), Some(packet(0, 1)));
        inbox.push(packet(0, 2), || {}).unwrap();
        assert_eq!(inbox.pop(), Some(packet(65, 1)));
        inbox.push(packet(65, 2), || {}).unwrap();
        assert_eq!(inbox.pop(), Some(packet(2, 1)));
        assert_eq!(inbox.pop(), Some(packet(63, 1)));
        assert_eq!(inbox.pop(), Some(packet(0, 2)));
        assert_eq!(inbox.pop(), Some(packet(65, 2)));
        assert_eq!(inbox.pop(), None);
        assert!(!inbox.has_pending());
        assert_eq!(inbox.pending(), 0);
    });
}

#[test]
fn both_lanes_preserve_payloads_before_producers_are_joined() {
    for routes in [[0, 2], [0, 63], [63, 64]] {
        model(move || {
            let inbox = Arc::new(WakeInbox::new(65));
            let writers = routes.map(|route| {
                let inbox = Arc::clone(&inbox);
                thread::spawn(move || inbox.push(packet(route, route as u64 + 1), || {}).unwrap())
            });
            let mut seen = [false; 2];
            let mut observe = |wake: WakePacket| {
                let index = routes
                    .iter()
                    .position(|route| *route == wake.route)
                    .unwrap();
                assert!(!seen[index], "route became runnable twice");
                assert_eq!(wake, packet(wake.route, wake.route as u64 + 1));
                seen[index] = true;
            };
            for _ in 0..2 {
                if let Some(wake) = inbox.pop() {
                    observe(wake);
                }
            }
            for writer in writers {
                assert!(!writer.join().unwrap());
            }
            while let Some(wake) = inbox.pop() {
                observe(wake);
            }
            assert_eq!(seen, [true; 2]);
            assert!(!inbox.has_pending());
        });
    }
}

#[test]
fn both_lanes_reject_reserved_routes_before_their_publication() {
    model(|| {
        let inbox = WakeInbox::new(64);
        for route in [0, 63] {
            let first = packet(route, 1);
            inbox
                .push(first, || {
                    assert_eq!(inbox.pending(), 1);
                    assert!(!inbox.has_pending());
                    assert!(inbox.push(packet(route, 2), || {}).is_err());
                })
                .unwrap();
            assert_eq!(inbox.pop(), Some(first));
            assert_eq!(inbox.pending(), 0);
        }
        assert!(inbox.push(packet(64, 1), || {}).is_err());
        assert!(inbox.push(packet(0, 0), || {}).is_err());
        assert_eq!(inbox.pending(), 0);
    });
}

#[test]
fn both_lanes_acknowledge_and_copy_before_releasing_a_route_for_reuse() {
    for route in [0, 63] {
        model(move || {
            let inbox = Arc::new(WakeInbox::new(64));
            inbox.push(packet(route, 1), || {}).unwrap();
            let writer = {
                let inbox = Arc::clone(&inbox);
                thread::spawn(move || inbox.push(packet(route, 2), || {}).is_ok())
            };
            assert_eq!(inbox.pop(), Some(packet(route, 1)));
            let reused = writer.join().unwrap();
            assert_eq!(inbox.has_pending(), reused);
            assert_eq!(inbox.pop(), reused.then(|| packet(route, 2)));
            assert_eq!(inbox.pop(), None);
        });
    }
}

#[test]
fn both_lanes_survive_arming_and_disarming_races() {
    for route in [0, 63] {
        model(move || {
            let inbox = Arc::new(WakeInbox::new(64));
            let writer = {
                let inbox = Arc::clone(&inbox);
                thread::spawn(move || inbox.push(packet(route, 1), || {}).unwrap())
            };
            let ready = inbox.arm_wait();
            assert!(writer.join().unwrap() || ready);
            inbox.disarm_wait();
            assert_eq!(inbox.pop(), Some(packet(route, 1)));
            assert!(!inbox.arm_wait());
            let writer = {
                let inbox = Arc::clone(&inbox);
                thread::spawn(move || inbox.push(packet(route, 2), || {}).unwrap())
            };
            inbox.disarm_wait();
            writer.join().unwrap();
            assert_eq!(inbox.pop(), Some(packet(route, 2)));
            assert_eq!(inbox.pop(), None);
        });
    }
}
