//! Poison-tolerant internal locks and lost-wakeup-free carrier notifications.

use std::sync::{
    Condvar, Mutex, MutexGuard,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::time::Instant;

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[derive(Default)]
pub(crate) struct Signal {
    epoch: AtomicU64,
    waiters: AtomicUsize,
    gate: Mutex<()>,
    changed: Condvar,
}

impl Signal {
    pub(crate) fn version(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub(crate) fn notify(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        // With sequential consistency, either this observes the registered
        // waiter or that waiter observes the new epoch before sleeping.
        if self.waiters.load(Ordering::SeqCst) != 0 {
            let _gate = lock(&self.gate);
            self.changed.notify_all();
        }
    }

    pub(crate) fn notify_if_waiting(&self) {
        if self.waiters.load(Ordering::SeqCst) != 0 {
            // Predicate-backed waits do not need an epoch change: the gate
            // handoff makes the published work visible without a lost wake,
            // and every inbox has exactly one owner carrier to notify.
            let _gate = lock(&self.gate);
            self.changed.notify_one();
        }
    }

    pub(crate) fn wait(&self, observed: u64, deadline: Option<Instant>) {
        self.wait_while(observed, deadline, || false);
    }

    pub(crate) fn wait_while(
        &self,
        observed: u64,
        deadline: Option<Instant>,
        mut ready: impl FnMut() -> bool,
    ) {
        let mut gate = lock(&self.gate);
        self.waiters.fetch_add(1, Ordering::SeqCst);
        while self.epoch.load(Ordering::SeqCst) == observed && !ready() {
            gate = if let Some(deadline) = deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                let (guard, _) = self
                    .changed
                    .wait_timeout(gate, remaining)
                    .unwrap_or_else(|poison| poison.into_inner());
                guard
            } else {
                self.changed
                    .wait(gate)
                    .unwrap_or_else(|poison| poison.into_inner())
            };
        }
        self.waiters.fetch_sub(1, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.waiters.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
#[path = "signal_test.rs"]
mod signal_test;
