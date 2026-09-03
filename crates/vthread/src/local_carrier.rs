//! Carrier-local admission and stack storage shared with mounted local-scope owners.

use crate::{Error, Result, RuntimeConfig, kernel_tasks::BorrowedTask};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use vthread_stack::StackPool;

pub(crate) struct LocalCarrier {
    starts: RefCell<VecDeque<BorrowedTask>>,
    pending_starts: Cell<usize>,
    pub(crate) stacks: RefCell<StackPool>,
    capacity: usize,
}

impl LocalCarrier {
    pub(crate) fn new(config: RuntimeConfig) -> Self {
        Self {
            starts: RefCell::new(VecDeque::new()),
            pending_starts: Cell::new(0),
            capacity: config.carrier_queue_capacity(),
            stacks: RefCell::new(StackPool::new(
                config.stack_size(),
                config.stack_cache_capacity(),
            )),
        }
    }
    pub(crate) fn check_capacity(&self) -> Result<()> {
        if self.pending_starts.get() >= self.capacity {
            Err(Error::Capacity {
                resource: crate::error::CapacityResource::CarrierQueue,
                limit: self.capacity,
            })
        } else {
            Ok(())
        }
    }

    pub(crate) fn push_start(&self, task: BorrowedTask) {
        self.starts.borrow_mut().push_back(task);
        self.pending_starts.set(self.pending_starts.get() + 1);
    }

    pub(crate) fn pop_start(&self) -> Option<BorrowedTask> {
        if self.pending_starts.get() == 0 {
            return None;
        }
        let task = self.starts.borrow_mut().pop_front();
        if task.is_some() {
            self.pending_starts.set(self.pending_starts.get() - 1);
        }
        task
    }

    pub(crate) fn take_starts(&self) -> VecDeque<BorrowedTask> {
        self.pending_starts.set(0);
        self.starts.take()
    }

    pub(crate) fn pending_starts(&self) -> usize {
        self.pending_starts.get()
    }
}

#[cfg(test)]
#[path = "local_carrier_test.rs"]
mod local_carrier_test;
