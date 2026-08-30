//! Bounded carrier-local stack cache.

use std::io;

use corosensei::stack::DefaultStack;

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
    snapshot: StackPoolSnapshot,
}

impl StackPool {
    /// Creates an empty pool.
    pub fn new(stack_size: usize, max_cached: usize) -> Self {
        Self {
            stack_size,
            max_cached,
            stacks: Vec::new(),
            snapshot: StackPoolSnapshot::default(),
        }
    }

    /// Acquires a cached stack or allocates a new one.
    pub fn acquire(&mut self) -> io::Result<DefaultStack> {
        if let Some(stack) = self.stacks.pop() {
            self.snapshot.cached = self.stacks.len();
            self.snapshot.reused += 1;
            return Ok(stack);
        }

        let stack = DefaultStack::new(self.stack_size)?;
        self.snapshot.allocated += 1;
        Ok(stack)
    }

    /// Returns a completed stack to the bounded cache.
    pub fn release(&mut self, stack: DefaultStack) {
        if self.stacks.len() < self.max_cached {
            self.stacks.push(stack);
            self.snapshot.retained += 1;
            self.snapshot.cached = self.stacks.len();
        } else {
            self.snapshot.discarded += 1;
        }
    }

    /// Returns the current counters.
    pub fn snapshot(&self) -> StackPoolSnapshot {
        self.snapshot
    }
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod pool_test;
