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
    // One route is sufficient for completion/registration/sleep ordering. Queue
    // occupancy and notice generation share a modeled word, not a new runtime
    // mailbox. Multi-route payload/list races are explicitly outside this model.
    queued: AtomicU64,
    pub(super) signal: Signal,
}

impl Route {
    const SLEEPING: u64 = 1 << 63;

    pub(super) fn new() -> Self {
        Self {
            queued: AtomicU64::new(0),
            signal: Signal::new(),
        }
    }

    pub(super) fn push(&self, generation: u64) {
        let mut before = self.queued.load(Ordering::Acquire);
        loop {
            assert_eq!(
                before & !Self::SLEEPING,
                0,
                "one outstanding notice per route"
            );
            match self.queued.compare_exchange(
                before,
                generation,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    if before & Self::SLEEPING != 0 {
                        self.signal.notify_if_waiting();
                    }
                    return;
                }
                Err(word) => before = word,
            }
        }
    }

    pub(super) fn pop(&self) -> Option<u64> {
        let generation = self.queued.swap(0, Ordering::Acquire) & !Self::SLEEPING;
        (generation != 0).then_some(generation)
    }

    fn arm(&self) -> bool {
        let mut before = self.queued.load(Ordering::Acquire);
        loop {
            if before & !Self::SLEEPING != 0 {
                return true;
            }
            if before == Self::SLEEPING {
                return false;
            }
            match self.queued.compare_exchange(
                before,
                Self::SLEEPING,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return false,
                Err(word) => before = word,
            }
        }
    }

    pub(super) fn wait(&self, observed: u64) {
        let mut gate = self.signal.gate.lock().unwrap();
        self.signal.waiters.fetch_add(1, Ordering::SeqCst);
        loom::sync::atomic::fence(Ordering::SeqCst);
        while self.signal.epoch.load(Ordering::SeqCst) == observed && !self.arm() {
            gate = self.signal.changed.wait(gate).unwrap();
        }
        self.signal.waiters.fetch_sub(1, Ordering::SeqCst);
        drop(gate);
        self.queued.fetch_and(!Self::SLEEPING, Ordering::Release);
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
