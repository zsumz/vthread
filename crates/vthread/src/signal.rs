//! Poison-tolerant internal locks and lost-wakeup-free carrier notifications.

use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Instant;

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[derive(Default)]
pub(crate) struct Signal {
    epoch: Mutex<u64>,
    changed: Condvar,
}

impl Signal {
    pub(crate) fn version(&self) -> u64 {
        *lock(&self.epoch)
    }

    pub(crate) fn notify(&self) {
        let mut epoch = lock(&self.epoch);
        *epoch = epoch.wrapping_add(1);
        self.changed.notify_all();
    }

    pub(crate) fn wait(&self, observed: u64, deadline: Option<Instant>) {
        let mut epoch = lock(&self.epoch);
        while *epoch == observed {
            epoch = if let Some(deadline) = deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return;
                }
                let (guard, _) = self
                    .changed
                    .wait_timeout(epoch, remaining)
                    .unwrap_or_else(|poison| poison.into_inner());
                guard
            } else {
                self.changed
                    .wait(epoch)
                    .unwrap_or_else(|poison| poison.into_inner())
            };
        }
    }
}

#[cfg(test)]
#[path = "signal_test.rs"]
mod signal_test;
