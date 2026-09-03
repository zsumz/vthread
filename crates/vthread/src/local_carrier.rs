//! Carrier-local admission and stack storage shared with mounted local-scope owners.

use crate::{Error, Result, RuntimeConfig, kernel_tasks::BorrowedTask, wait::WakeNotice};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use vthread_stack::{ParkToken, StackPool};

pub(crate) struct LocalCarrier {
    starts: RefCell<VecDeque<BorrowedTask>>,
    pending_starts: Cell<usize>,
    wakes: RefCell<VecDeque<WakeNotice>>,
    pending_wakes: Cell<usize>,
    pub(crate) stacks: RefCell<StackPool>,
    capacity: usize,
}

impl LocalCarrier {
    pub(crate) fn new(config: RuntimeConfig) -> Self {
        Self {
            starts: RefCell::new(VecDeque::new()),
            pending_starts: Cell::new(0),
            wakes: RefCell::new(VecDeque::new()),
            pending_wakes: Cell::new(0),
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

    pub(crate) fn push_wake(&self, notice: WakeNotice) {
        self.wakes.borrow_mut().push_back(notice);
        self.pending_wakes.set(self.pending_wakes.get() + 1);
    }

    pub(crate) fn pop_wake(&self) -> Option<WakeNotice> {
        let notice = self.wakes.borrow_mut().pop_front();
        if notice.is_some() {
            self.pending_wakes.set(self.pending_wakes.get() - 1);
        }
        notice
    }

    pub(crate) fn unregister_wake(&self, token: ParkToken) {
        let mut wakes = self.wakes.borrow_mut();
        let previous = wakes.len();
        wakes.retain(|notice| notice.token != token);
        self.pending_wakes
            .set(self.pending_wakes.get() - (previous - wakes.len()));
    }

    pub(crate) fn pending_wakes(&self) -> usize {
        self.pending_wakes.get()
    }
}

#[cfg(test)]
#[path = "local_carrier_test.rs"]
mod local_carrier_test;
