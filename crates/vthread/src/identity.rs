//! Opaque process and runtime-local diagnostic identities.
use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_RUNTIME: AtomicU64 = AtomicU64::new(1);
/// Process-unique identity; it is not a persistent identifier across process restarts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeId(u64);
impl RuntimeId {
    pub(crate) fn next() -> Self {
        Self(
            NEXT_RUNTIME
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .expect("runtime identity space exhausted"),
        )
    }
}
impl fmt::Display for RuntimeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
/// Owned root or supervisor identity within one runtime.
/// Pair it with `RuntimeId` for process-wide correlation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(u64);
impl ScopeId {
    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }
}
impl fmt::Display for ScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
#[cfg(test)]
#[path = "identity_test.rs"]
mod identity_test;
