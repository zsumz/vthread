//! Bounded carrier-local stack cache.

#[cfg(feature = "runtime-evidence")]
use std::collections::BTreeMap;
use std::io;

use corosensei::stack::DefaultStack;
#[cfg(feature = "runtime-evidence")]
use corosensei::stack::Stack;

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
pub struct StackPool {
    stack_size: usize,
    max_cached: usize,
    stacks: Vec<DefaultStack>,
    #[cfg(feature = "runtime-evidence")]
    identities: BTreeMap<usize, u64>,
    #[cfg(feature = "runtime-evidence")]
    mappings: BTreeMap<u64, usize>,
    #[cfg(feature = "runtime-evidence")]
    next_identity: u64,
    snapshot: StackPoolSnapshot,
}

impl StackPool {
    /// Creates an empty pool.
    pub fn new(stack_size: usize, max_cached: usize) -> Self {
        Self {
            stack_size,
            max_cached,
            stacks: Vec::new(),
            #[cfg(feature = "runtime-evidence")]
            identities: BTreeMap::new(),
            #[cfg(feature = "runtime-evidence")]
            mappings: BTreeMap::new(),
            #[cfg(feature = "runtime-evidence")]
            next_identity: 1,
            snapshot: StackPoolSnapshot::default(),
        }
    }

    /// Acquires a cached stack or allocates a new one.
    pub fn acquire(&mut self) -> io::Result<DefaultStack> {
        #[cfg(feature = "runtime-evidence")]
        {
            self.acquire_identified().map(|(_, stack)| stack)
        }
        #[cfg(not(feature = "runtime-evidence"))]
        {
            if let Some(stack) = self.stacks.pop() {
                self.snapshot.cached = self.stacks.len();
                self.snapshot.reused += 1;
                return Ok(stack);
            }
            self.snapshot.allocated += 1;
            DefaultStack::new(self.stack_size)
        }
    }

    /// Acquires a stack together with its stable pool-local mapping identity.
    #[cfg(feature = "runtime-evidence")]
    pub fn acquire_identified(&mut self) -> io::Result<(u64, DefaultStack)> {
        if let Some(stack) = self.stacks.pop() {
            self.snapshot.cached = self.stacks.len();
            self.snapshot.reused += 1;
            let identity = self
                .identity(&stack)
                .ok_or_else(|| io::Error::other("cached stack mapping has no pool identity"))?;
            return Ok((identity, stack));
        }

        let stack = DefaultStack::new(self.stack_size)?;
        let identity = self.next_identity;
        self.next_identity = identity
            .checked_add(1)
            .ok_or_else(|| io::Error::other("stack mapping identity exhausted"))?;
        let base = stack.base().get();
        self.identities.insert(base, identity);
        self.mappings.insert(identity, base);
        self.snapshot.allocated += 1;
        Ok((identity, stack))
    }

    /// Returns a completed stack to the bounded cache.
    pub fn release(&mut self, stack: DefaultStack) {
        #[cfg(feature = "runtime-evidence")]
        {
            let Some(identity) = self.identity(&stack) else {
                self.snapshot.discarded += 1;
                return;
            };
            self.release_identified(identity, stack);
        }
        #[cfg(not(feature = "runtime-evidence"))]
        {
            if self.stacks.len() < self.max_cached {
                self.stacks.push(stack);
                self.snapshot.retained += 1;
                self.snapshot.cached = self.stacks.len();
            } else {
                self.snapshot.discarded += 1;
            }
        }
    }

    /// Returns an identified mapping and reports whether the bounded cache retained it.
    #[cfg(feature = "runtime-evidence")]
    pub fn release_identified(&mut self, identity: u64, stack: DefaultStack) -> bool {
        let actual = self.identity(&stack);
        if actual != Some(identity) {
            self.snapshot.discarded += 1;
            if let Some(actual) = actual {
                self.retire(actual);
            }
            return false;
        }
        if self.stacks.len() < self.max_cached {
            self.stacks.push(stack);
            self.snapshot.retained += 1;
            self.snapshot.cached = self.stacks.len();
            true
        } else {
            self.snapshot.discarded += 1;
            self.retire(identity);
            false
        }
    }

    /// Retires metadata after its active mapping was discarded outside the cache.
    #[cfg(feature = "runtime-evidence")]
    pub fn retire(&mut self, identity: u64) -> bool {
        let Some(base) = self.mappings.remove(&identity) else {
            return false;
        };
        self.identities.remove(&base);
        true
    }

    /// Returns the current counters.
    pub fn snapshot(&self) -> StackPoolSnapshot {
        self.snapshot
    }

    #[cfg(feature = "runtime-evidence")]
    fn identity(&self, stack: &DefaultStack) -> Option<u64> {
        self.identities.get(&stack.base().get()).copied()
    }
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod pool_test;
