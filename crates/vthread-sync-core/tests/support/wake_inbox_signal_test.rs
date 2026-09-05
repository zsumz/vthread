//! Composition with Signal's predicate wait path; unrelated epoch changes are absent.

use loom::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use crate::{WakeInbox, WakePacket, wake_atomic::model};

struct PredicateWait {
    inbox: WakeInbox,
    waiters: AtomicUsize,
    gate: Mutex<()>,
    changed: Condvar,
}

impl PredicateWait {
    fn new() -> Self {
        Self {
            inbox: WakeInbox::new(64),
            waiters: AtomicUsize::new(0),
            gate: Mutex::new(()),
            changed: Condvar::new(),
        }
    }

    fn publish(&self, route: usize) {
        let packet = WakePacket {
            route,
            task: route as u64 + 1,
            wait: 27,
            selection: 81,
        };
        // WaitHub::push -> Signal::notify_if_waiting. The registered waiter must
        // be visible when publication reports that its owner is armed for sleep.
        if self.inbox.push(packet, || {}).unwrap() && self.waiters.load(Ordering::SeqCst) != 0 {
            let _gate = self.gate.lock().unwrap();
            self.changed.notify_one();
        }
    }

    fn wait(&self) {
        // Signal::wait_while with an unchanged epoch and no timeout, followed by
        // WaitHub::wait's disarm. Keep this ordering aligned with those methods.
        {
            let mut gate = self.gate.lock().unwrap();
            self.waiters.fetch_add(1, Ordering::SeqCst);
            while !self.inbox.arm_wait() {
                gate = self.changed.wait(gate).unwrap();
            }
            self.waiters.fetch_sub(1, Ordering::SeqCst);
        }
        self.inbox.disarm_wait();
    }
}

#[test]
fn either_lane_releases_a_real_predicate_wait_without_an_epoch_change() {
    for routes in [vec![0], vec![63], vec![0, 63]] {
        model(move || {
            let signal = Arc::new(PredicateWait::new());
            let writers = routes
                .iter()
                .map(|&route| {
                    let signal = Arc::clone(&signal);
                    thread::spawn(move || signal.publish(route))
                })
                .collect::<Vec<_>>();
            signal.wait();
            for writer in writers {
                writer.join().unwrap();
            }
            let mut observed = Vec::new();
            while let Some(packet) = signal.inbox.pop() {
                assert_eq!(packet.task, packet.route as u64 + 1);
                assert_eq!(packet.wait, 27);
                assert_eq!(packet.selection, 81);
                observed.push(packet.route);
            }
            observed.sort_unstable();
            assert_eq!(observed, routes);
        });
    }
}
