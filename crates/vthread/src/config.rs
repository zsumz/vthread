//! Runtime capacity and stack configuration.

use crate::{Error, Result, Runtime};

const DEFAULT_MAX_VTHREADS: usize = 65_536;
const DEFAULT_STACK_SIZE: usize = 1024 * 1024;
const DEFAULT_STACK_CACHE: usize = 64;
const MIN_STACK_SIZE: usize = 64 * 1024;

/// Immutable runtime configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeConfig {
    max_vthreads: usize,
    stack_size: usize,
    stack_cache_capacity: usize,
}

impl RuntimeConfig {
    /// Maximum number of live virtual threads.
    pub fn max_vthreads(self) -> usize {
        self.max_vthreads
    }

    /// Reserved stack capacity requested for each virtual thread.
    pub fn stack_size(self) -> usize {
        self.stack_size
    }

    /// Maximum completed stacks retained for reuse.
    pub fn stack_cache_capacity(self) -> usize {
        self.stack_cache_capacity
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_vthreads: DEFAULT_MAX_VTHREADS,
            stack_size: DEFAULT_STACK_SIZE,
            stack_cache_capacity: DEFAULT_STACK_CACHE,
        }
    }
}

/// Builder for a single-carrier runtime.
#[derive(Clone, Copy, Debug, Default)]
pub struct RuntimeBuilder {
    config: RuntimeConfig,
}

impl RuntimeBuilder {
    /// Sets the maximum number of live virtual threads.
    pub fn max_vthreads(mut self, limit: usize) -> Self {
        self.config.max_vthreads = limit;
        self
    }

    /// Sets the requested stack capacity per virtual thread.
    pub fn stack_size(mut self, bytes: usize) -> Self {
        self.config.stack_size = bytes;
        self
    }

    /// Sets the number of completed stacks retained by the runtime.
    pub fn stack_cache_capacity(mut self, capacity: usize) -> Self {
        self.config.stack_cache_capacity = capacity;
        self
    }

    /// Validates the configuration and constructs a runtime.
    pub fn build(self) -> Result<Runtime> {
        if self.config.max_vthreads == 0 {
            return Err(Error::invalid_configuration(
                "max_vthreads",
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
        Ok(Runtime::from_config(self.config))
    }
}

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;
