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

    pub(crate) fn wait(&self, observed: u64, deadline: Option<Instant>) {
        let mut gate = lock(&self.gate);
        self.waiters.fetch_add(1, Ordering::SeqCst);
        while self.epoch.load(Ordering::SeqCst) == observed {
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
}

#[cfg(test)]
#[path = "signal_test.rs"]
mod signal_test;
