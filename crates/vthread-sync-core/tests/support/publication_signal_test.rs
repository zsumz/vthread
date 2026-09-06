//! One bounded route and the existing Signal sleep protocol through Loom types.
//!
//! Loom 0.7.2 weakens SeqCst accesses to AcqRel (its README, Unsupported features).
//! The two model-only fences below enforce the SC store-buffering obligation:
//! either notify sees a registered waiter or the waiter sees the advanced epoch.
//! A separate exhaustive SC-order test checks that obligation. These are adapter
//! fences, not a proposed production ordering change or full native Signal proof.

use loom::sync::{
    Condvar, Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

pub(super) struct Signal {
    epoch: AtomicU64,
    waiters: AtomicUsize,
    gate: Mutex<()>,
    changed: Condvar,
}

impl Signal {
    fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            waiters: AtomicUsize::new(0),
            gate: Mutex::new(()),
            changed: Condvar::new(),
        }
    }

    pub(super) fn version(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub(super) fn notify(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        loom::sync::atomic::fence(Ordering::SeqCst);
        if self.waiters.load(Ordering::SeqCst) != 0 {
            let _gate = self.gate.lock().unwrap();
            self.changed.notify_all();
        }
    }

    fn notify_if_waiting(&self) {
        if self.waiters.load(Ordering::SeqCst) != 0 {
            let _gate = self.gate.lock().unwrap();
            self.changed.notify_one();
        }
    }
}

pub(super) struct Route {
    pub(super) queue: super::wake_queue_core::WakeQueue,
    pub(super) signal: Signal,
}

impl Route {
    pub(super) fn new() -> Self {
        Self {
            queue: super::wake_queue_core::WakeQueue::new(2),
            signal: Signal::new(),
        }
    }

    pub(super) fn publication_complete(&self) {
        self.signal.notify();
    }

    pub(super) fn push(&self, notice: super::WakeNotice) {
        if self.queue.push(notice, || {}).unwrap() {
            self.signal.notify_if_waiting();
        }
    }

    pub(super) fn pop(&self) -> Option<u64> {
        self.queue.pop().map(|notice| notice.token.generation())
    }

    pub(super) fn wait(&self, observed: u64) {
        let mut gate = self.signal.gate.lock().unwrap();
        self.signal.waiters.fetch_add(1, Ordering::SeqCst);
        loom::sync::atomic::fence(Ordering::SeqCst);
        while self.signal.epoch.load(Ordering::SeqCst) == observed && !self.queue.arm_wait() {
            gate = self.signal.changed.wait(gate).unwrap();
        }
        self.signal.waiters.fetch_sub(1, Ordering::SeqCst);
        drop(gate);
        self.queue.disarm_wait();
    }
}

#[test]
fn sequential_consistency_forbids_both_notification_observers_missing() {
    // W=register waiter, E=recheck epoch, P=advance epoch, N=check waiters.
    // SC preserves W<E and P<N. Enumerate all six legal total orders, keeping
    // each load separate; do not turn the whole protocol into a mutex action.
    let mut orders = 0;
    for first in 0..4 {
        for second in 0..4 {
            for third in 0..4 {
                for fourth in 0..4 {
                    let order = [first, second, third, fourth];
                    if (0..4).any(|event| order.iter().filter(|&&e| e == event).count() != 1)
                        || order.iter().position(|&e| e == 0) > order.iter().position(|&e| e == 1)
                        || order.iter().position(|&e| e == 2) > order.iter().position(|&e| e == 3)
                    {
                        continue;
                    }
                    let (mut registered, mut advanced) = (false, false);
                    let (mut saw_epoch, mut saw_waiter) = (false, false);
                    for event in order {
                        match event {
                            0 => registered = true,
                            1 => saw_epoch = advanced,
                            2 => advanced = true,
                            3 => saw_waiter = registered,
                            _ => unreachable!(),
                        }
                    }
                    assert!(saw_epoch || saw_waiter, "SC lost-wake order: {order:?}");
                    orders += 1;
                }
            }
        }
    }
    assert_eq!(orders, 6);
}
