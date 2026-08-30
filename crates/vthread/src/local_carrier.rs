//! Carrier-local admission and stack storage shared with mounted local-scope owners.

use crate::{Error, Result, RuntimeConfig, kernel::Task};
use std::{cell::RefCell, collections::VecDeque};
use vthread_stack::StackPool;

pub(crate) struct LocalCarrier {
    pub(crate) starts: RefCell<VecDeque<Task>>,
    pub(crate) stacks: RefCell<StackPool>,
    capacity: usize,
}

impl LocalCarrier {
    pub(crate) fn new(config: RuntimeConfig) -> Self {
        Self {
            starts: RefCell::new(VecDeque::new()),
            capacity: config.carrier_queue_capacity(),
            stacks: RefCell::new(StackPool::new(
                config.stack_size(),
                config.stack_cache_capacity(),
            )),
        }
    }
    pub(crate) fn check_capacity(&self) -> Result<()> {
        if self.starts.borrow().len() >= self.capacity {
            Err(Error::CarrierQueueFull)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "local_carrier_test.rs"]
mod local_carrier_test;
