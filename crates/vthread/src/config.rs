//! Runtime capacity and stack configuration.

use crate::{Error, Result, Runtime};
use std::time::{Duration, Instant};

const DEFAULT_MAX_VTHREADS: usize = 65_536;
const DEFAULT_STACK_SIZE: usize = 1024 * 1024;
const DEFAULT_STACK_CACHE: usize = 64;
const MIN_STACK_SIZE: usize = 64 * 1024;

/// Immutable runtime configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeConfig {
    io_capacity: usize,
    blocking_threads: usize,
    blocking_capacity: usize,
    max_vthreads: usize,
    stack_size: usize,
    stack_cache_capacity: usize,
    carriers: usize,
    task_local_capacity: usize,
    carrier_queue_capacity: usize,
    stall_timeout: Option<Duration>,
}

impl RuntimeConfig {
    /// Maximum outstanding readiness registrations.
    pub fn io_capacity(self) -> usize {
        self.io_capacity
    }
    /// Number of dedicated native blocking workers.
    pub fn blocking_threads(self) -> usize {
        self.blocking_threads
    }
    /// Maximum queued, running, and stopped-job cleanup operations combined.
    pub fn blocking_capacity(self) -> usize {
        self.blocking_capacity
    }
    /// Maximum initialized task-local keys per virtual thread.
    pub fn task_local_capacity(self) -> usize {
        self.task_local_capacity
    }

    /// Number of persistent carrier threads.
    pub fn carriers(self) -> usize {
        self.carriers
    }

    /// Maximum unstarted packets queued per carrier.
    pub fn carrier_queue_capacity(self) -> usize {
        self.carrier_queue_capacity
    }

    /// Grace period for an entirely parked, timerless scope; None disables recovery.
    pub fn stall_timeout(self) -> Option<Duration> {
        self.stall_timeout
    }
    /// Maximum live tasks plus unobserved completion records.
    pub fn max_vthreads(self) -> usize {
        self.max_vthreads
    }

    /// Reserved stack capacity requested for each virtual thread.
    pub fn stack_size(self) -> usize {
        self.stack_size
    }

    /// Maximum completed stacks retained for reuse per carrier.
    pub fn stack_cache_capacity(self) -> usize {
        self.stack_cache_capacity
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            io_capacity: 1024,
            blocking_threads: 2,
            blocking_capacity: 256,
            max_vthreads: DEFAULT_MAX_VTHREADS,
            stack_size: DEFAULT_STACK_SIZE,
            stack_cache_capacity: DEFAULT_STACK_CACHE,
            carriers: 1,
            task_local_capacity: 64,
            carrier_queue_capacity: 256,
            stall_timeout: Some(Duration::from_secs(1)),
        }
    }
}

/// Builder for a bounded multicarrier runtime.
#[derive(Clone, Copy, Debug, Default)]
pub struct RuntimeBuilder {
    config: RuntimeConfig,
}

impl RuntimeBuilder {
    /// Bounds readiness waits across this runtime.
    pub fn io_capacity(mut self, capacity: usize) -> Self {
        self.config.io_capacity = capacity;
        self
    }
    /// Sets a positive dedicated native worker count.
    pub fn blocking_threads(mut self, threads: usize) -> Self {
        self.config.blocking_threads = threads;
        self
    }
    /// Bounds queued, running, and stopped-job cleanup work; excess work is rejected.
    pub fn blocking_capacity(mut self, capacity: usize) -> Self {
        self.config.blocking_capacity = capacity;
        self
    }
    /// Bounds initialized task-local keys per virtual thread.
    pub fn task_local_capacity(mut self, capacity: usize) -> Self {
        self.config.task_local_capacity = capacity;
        self
    }

    /// Sets the number of persistent carriers; started tasks never migrate.
    pub fn carriers(mut self, count: usize) -> Self {
        self.config.carriers = count;
        self
    }

    /// Sets the bounded unstarted-packet capacity of each carrier inbox.
    pub fn carrier_queue_capacity(mut self, capacity: usize) -> Self {
        self.config.carrier_queue_capacity = capacity;
        self
    }

    /// Sets quiescent-scope recovery; use None for indefinite external wake waits.
    pub fn stall_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.config.stall_timeout = timeout;
        self
    }
    /// Bounds live tasks and unobserved completions; joined records may be evicted.
    pub fn max_vthreads(mut self, limit: usize) -> Self {
        self.config.max_vthreads = limit;
        self
    }

    /// Sets the requested stack capacity per virtual thread.
    pub fn stack_size(mut self, bytes: usize) -> Self {
        self.config.stack_size = bytes;
        self
    }

    /// Sets the number of completed stacks retained per carrier.
    pub fn stack_cache_capacity(mut self, capacity: usize) -> Self {
        self.config.stack_cache_capacity = capacity;
        self
    }

    /// Validates the configuration and constructs a runtime.
    pub fn build(self) -> Result<Runtime> {
        if self.config.io_capacity == 0 {
            return Err(Error::invalid_configuration(
                "io_capacity",
                "must be positive",
            ));
        }
        if self.config.blocking_threads == 0
            || self.config.blocking_threads > self.config.blocking_capacity
        {
            return Err(Error::invalid_configuration(
                "blocking_threads",
                "must be between one and blocking_capacity",
            ));
        }
        if self
            .config
            .stall_timeout
            .is_some_and(|timeout| Instant::now().checked_add(timeout).is_none())
        {
            return Err(Error::invalid_configuration(
                "stall_timeout",
                "must fit the monotonic clock",
            ));
        }
        if self.config.max_vthreads == 0 {
            return Err(Error::invalid_configuration(
                "max_vthreads",
                "must be greater than zero",
            ));
        }
        if self.config.carriers == 0 || self.config.carriers > self.config.max_vthreads {
            return Err(Error::invalid_configuration(
                "carriers",
                "must be between one and max_vthreads",
            ));
        }
        if self.config.carrier_queue_capacity == 0 {
            return Err(Error::invalid_configuration(
                "carrier_queue_capacity",
                "must be greater than zero",
            ));
        }
        if self.config.stack_size < MIN_STACK_SIZE {
            return Err(Error::invalid_configuration(
                "stack_size",
                "must be at least 64 KiB",
            ));
        }
        if self.config.stack_cache_capacity > self.config.max_vthreads {
            return Err(Error::invalid_configuration(
                "stack_cache_capacity",
                "cannot exceed max_vthreads",
            ));
        }
        Runtime::from_config(self.config)
    }
}

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;
