//! Runtime capacity and stack configuration.

use crate::{Error, Result, Runtime, StallPolicy};
use std::time::Instant;

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
    stall_policy: StallPolicy,
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
    /// Maximum queued, running, completed-but-unclaimed and native-disposal jobs combined.
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

    /// Explicit inactivity policy; disabled by default because external wakes may be delayed.
    pub fn stall_policy(self) -> StallPolicy {
        self.stall_policy
    }
    /// Maximum live tasks plus retained completions; also the independent limit on
    /// active owned scope records (roots and supervisors). Local groups reuse their
    /// enclosing owned scope; their children consume task capacity. Each count uses
    /// this same limit; owned scope exhaustion reports `CapacityResource::Scopes`.
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
            stall_policy: StallPolicy::Disabled,
        }
    }
}

/// Builder for a bounded multicarrier runtime.
///
/// Construction starts `carriers + blocking_threads + 2` OS threads: affine carriers,
/// native workers, one readiness driver, and one shutdown coordinator (five by default).
/// One process-wide lifecycle owner is shared. Coordinator/owner stacks request 256 KiB;
/// other native stacks use platform defaults. The owner polls completions every 10 ms.
///
/// One root may run at a time; supervisors and local task groups reuse these threads.
/// [`crate::lifecycle::LIFECYCLE_CAPACITY`] limits retained lifecycles to 256. A full table
/// returns [`Error::Capacity`] with [`crate::error::CapacityResource::Lifecycles`]; a failed
/// process owner returns [`Error::LifecycleFailed`] before runtime workers start.
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
    /// Bounds queued, running, completed-but-unclaimed and native-disposal jobs combined.
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

    /// Chooses disabled, report-only, or explicit abort behavior for inactive root scopes.
    pub fn stall_policy(mut self, policy: StallPolicy) -> Self {
        self.config.stall_policy = policy;
        self
    }
    /// Bounds live tasks and unobserved completions; joined records may be evicted.
    /// Also independently bounds active owned scope records (roots and supervisors).
    /// Local groups reuse their enclosing owned scope; their children consume task
    /// capacity. Owned scope saturation reports `CapacityResource::Scopes`.
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
    /// Native/OS initialization, including readiness startup, has no elapsed-time bound.
    /// Use a process-level startup watchdog where bounded service startup is required.
    /// Explicitly rolls back partial initialization. Returns the construction error
    /// directly if cleanup succeeds, or [`Error::ConstructionFailed`] with both causes.
    pub fn build(self) -> Result<Runtime> {
        if self.config.io_capacity == 0 {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::IoCapacity,
                "must be positive",
            ));
        }
        if self.config.blocking_capacity == 0 {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::BlockingCapacity,
                "must be positive",
            ));
        }
        if self.config.blocking_threads == 0 {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::BlockingThreads,
                "must be positive",
            ));
        }
        if self.config.blocking_threads > self.config.blocking_capacity {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::BlockingThreads,
                "cannot exceed blocking_capacity",
            ));
        }
        if self
            .config
            .stall_policy
            .timeout()
            .is_some_and(|timeout| Instant::now().checked_add(timeout).is_none())
        {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::StallPolicy,
                "must fit the monotonic clock",
            ));
        }
        if self.config.max_vthreads == 0 {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::MaxVthreads,
                "must be greater than zero",
            ));
        }
        if self.config.carriers == 0 || self.config.carriers > self.config.max_vthreads {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::Carriers,
                "must be between one and max_vthreads",
            ));
        }
        if self.config.carrier_queue_capacity == 0 {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::CarrierQueueCapacity,
                "must be greater than zero",
            ));
        }
        if self.config.stack_size < MIN_STACK_SIZE {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::StackSize,
                "must be at least 64 KiB",
            ));
        }
        if self.config.stack_cache_capacity > self.config.max_vthreads {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::StackCacheCapacity,
                "cannot exceed max_vthreads",
            ));
        }
        Runtime::from_config(self.config)
    }
}

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;
