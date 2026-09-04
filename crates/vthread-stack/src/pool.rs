//! Bounded carrier-local stack cache.

#[cfg(feature = "runtime-evidence")]
use std::collections::BTreeSet;
use std::io;

use crate::MappedStack;

/// Operational counters for a stack pool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StackPoolSnapshot {
    /// Stacks currently retained for reuse.
    pub cached: usize,
    /// Fresh stack mappings created.
    pub allocated: u64,
    /// Cached stacks handed out again.
    pub reused: u64,
    /// Completed stacks accepted into the cache.
    pub retained: u64,
    /// Completed stacks dropped because the cache was full.
    pub discarded: u64,
}

/// A bounded cache of guard-page-backed stacks.
///
/// Every mapping the pool allocates is stamped with a pool-local identity that survives
/// cache reuse and is retired once the mapping leaves the pool for good.
pub struct StackPool {
    stack_size: usize,
    max_cached: usize,
    stacks: Vec<MappedStack>,
    next_identity: u64,
    #[cfg(feature = "runtime-evidence")]
    live: BTreeSet<u64>,
    snapshot: StackPoolSnapshot,
}

impl StackPool {
    /// Creates an empty pool.
    pub fn new(stack_size: usize, max_cached: usize) -> Self {
        Self {
            stack_size,
            max_cached,
            stacks: Vec::new(),
            next_identity: 1,
            #[cfg(feature = "runtime-evidence")]
            live: BTreeSet::new(),
            snapshot: StackPoolSnapshot::default(),
        }
    }

    /// Acquires a cached stack or allocates a new one.
    pub fn acquire(&mut self) -> io::Result<MappedStack> {
        if let Some(stack) = self.reuse() {
            return Ok(stack);
        }
        self.allocate()
    }

    /// Acquires a stack together with its stable pool-local mapping identity.
    #[cfg(feature = "runtime-evidence")]
    pub fn acquire_identified(&mut self) -> io::Result<(u64, MappedStack)> {
        let stack = self.acquire()?;
        Ok((stack.identity(), stack))
    }

    /// Returns a completed stack to the bounded cache.
    pub fn release(&mut self, stack: MappedStack) {
        #[cfg(feature = "runtime-evidence")]
        {
            let identity = stack.identity();
            self.release_identified(identity, stack);
        }
        #[cfg(not(feature = "runtime-evidence"))]
        self.retain(stack);
    }

    /// Returns an identified mapping and reports whether the bounded cache retained it.
    ///
    /// A mapping whose stamp differs from `identity`, or that this pool never issued or
    /// has already retired, is dropped instead of cached.
    #[cfg(feature = "runtime-evidence")]
    pub fn release_identified(&mut self, identity: u64, stack: MappedStack) -> bool {
        let actual = stack.identity();
        if actual != identity || !self.live.contains(&actual) {
            self.snapshot.discarded += 1;
            self.retire(actual);
            drop(stack);
            return false;
        }
        if self.retain(stack) {
            true
        } else {
            self.retire(identity);
            false
        }
    }

    /// Retires metadata after its active mapping was discarded outside the cache.
    #[cfg(feature = "runtime-evidence")]
    pub fn retire(&mut self, identity: u64) -> bool {
        self.live.remove(&identity)
    }

    /// Returns the current counters.
    pub fn snapshot(&self) -> StackPoolSnapshot {
        self.snapshot
    }

    fn reuse(&mut self) -> Option<MappedStack> {
        let stack = self.stacks.pop()?;
        self.snapshot.cached = self.stacks.len();
        self.snapshot.reused += 1;
        Some(stack)
    }

    fn allocate(&mut self) -> io::Result<MappedStack> {
        let identity = self.next_identity;
        let next = identity
            .checked_add(1)
            .ok_or_else(|| io::Error::other("stack mapping identity exhausted"))?;
        let stack = MappedStack::new(self.stack_size, identity)?;
        self.next_identity = next;
        #[cfg(feature = "runtime-evidence")]
        self.live.insert(identity);
        self.snapshot.allocated += 1;
        Ok(stack)
    }

    fn retain(&mut self, stack: MappedStack) -> bool {
        if self.stacks.len() < self.max_cached {
            self.stacks.push(stack);
            self.snapshot.retained += 1;
            self.snapshot.cached = self.stacks.len();
            true
        } else {
            self.snapshot.discarded += 1;
            false
        }
    }
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod pool_test;
