//! Atomic adapter shared with the standalone Loom test harness.

pub(crate) use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
pub(crate) use std::{sync::Arc, thread};

#[cfg(test)]
pub(crate) fn model(f: impl Fn() + Send + Sync + 'static) {
    f();
}

#[cfg(test)]
#[path = "wake_atomic_test.rs"]
mod wake_atomic_test;
