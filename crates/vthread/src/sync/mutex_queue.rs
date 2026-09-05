//! Bounded FIFO selection and cancellation-safe ownership transfer for virtual mutexes.

use super::wait::Wait;
use crate::{
    Error, Result,
    wait::{ResourceSelection, WaitCell},
};
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicUsize, Ordering},
};
use vthread_sync_core::{
    ExclusiveCell, ExclusiveGuard, Ownership, OwnershipSlot, QueueDecision, SpinMutex,
};

pub(super) struct MutexQueue {
    outstanding: AtomicUsize,
    capacity: usize,
    entries: SpinMutex<VecDeque<WaitCell>>,
    handoff: OwnershipSlot,
    #[cfg(test)]
    pub(super) after_dequeue: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

pub(super) enum Subscription<'mutex, 'wait, T> {
    Acquired(ExclusiveGuard<'mutex, T>),
    Waiting(Ticket<'mutex, 'wait, T>),
}

pub(super) struct Ticket<'mutex, 'wait, T> {
    queue: &'mutex MutexQueue,
    value: &'mutex ExclusiveCell<T>,
    wait: Option<&'wait WaitCell>,
    selected: bool,
}

impl MutexQueue {
    pub(super) fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::WaitCapacity,
                "must be positive",
            ));
        }
        Ok(Self {
            outstanding: AtomicUsize::new(0),
            capacity,
            entries: SpinMutex::new(VecDeque::new()),
            handoff: OwnershipSlot::new(),
            #[cfg(test)]
            after_dequeue: std::sync::Mutex::new(None),
        })
    }

    pub(super) fn subscribe<'mutex, 'wait, T>(
        &'mutex self,
        value: &'mutex ExclusiveCell<T>,
        wait: &'wait WaitCell,
    ) -> Result<Subscription<'mutex, 'wait, T>> {
        self.reserve()?;
        let mut entries = self.entries.lock();
        match value.queue_or_lock() {
            QueueDecision::Acquired(guard) => {
                assert!(entries.is_empty(), "unlocked mutex retained waiters");
                assert!(
                    self.handoff.take().is_none(),
                    "unlocked mutex retained ownership"
                );
                drop(entries);
                self.retire();
                Ok(Subscription::Acquired(guard))
            }
            QueueDecision::Queued => {
                entries.push_back(wait.clone());
                Ok(Subscription::Waiting(Ticket {
                    queue: self,
                    value,
                    wait: Some(wait),
                    selected: false,
                }))
            }
        }
    }

    pub(super) fn release<T>(&self, value: &ExclusiveCell<T>, guard: ExclusiveGuard<'_, T>) {
        let Err(ownership) = guard.try_release() else {
            return;
        };
        self.release_ownership(value, ownership);
    }

    pub(super) fn release_ownership<T>(&self, value: &ExclusiveCell<T>, mut ownership: Ownership) {
        loop {
            let publication = {
                let mut entries = self.entries.lock();
                let Some(wait) = entries.pop_front() else {
                    assert!(
                        self.handoff.take().is_none(),
                        "mutex retained two selected owners"
                    );
                    assert!(value.unlock(ownership).is_ok(), "mutex ownership mismatch");
                    return;
                };
                assert!(
                    value.set_waiters(&ownership, !entries.is_empty()),
                    "mutex ownership mismatch"
                );
                assert!(
                    self.handoff.publish(ownership).is_ok(),
                    "mutex retained two selected owners"
                );
                // Cancellation cleanup takes this same queue lock. Reserve the
                // recipient before exposing its removal, so cleanup either
                // removes a queued ticket or observes its ownership grant.
                wait.reserve_resource(ResourceSelection::Permit)
            };
            #[cfg(test)]
            let hook = crate::signal::lock(&self.after_dequeue).take();
            #[cfg(test)]
            if let Some(hook) = hook {
                hook();
            }
            if let Some(publication) = publication {
                publication.publish();
                return;
            }
            ownership = self.handoff.take().expect("rejected mutex owner");
        }
    }

    pub(super) fn waiting(&self) -> usize {
        self.outstanding.load(Ordering::Acquire)
    }

    pub(super) const fn wait_capacity(&self) -> usize {
        self.capacity
    }

    fn reserve(&self) -> Result<()> {
        if self
            .outstanding
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |outstanding| {
                (outstanding < self.capacity).then_some(outstanding + 1)
            })
            .is_err()
        {
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::Waiters,
                limit: self.capacity,
            });
        }
        Ok(())
    }

    fn retire(&self) {
        let previous = self.outstanding.fetch_sub(1, Ordering::AcqRel);
        assert!(previous != 0, "mutex ticket released twice");
    }
}

impl<T> Ticket<'_, '_, T> {
    pub(super) fn wait(mut self, wait: &Wait) -> Result<Ownership> {
        let wait_cell = self.wait.expect("live mutex ticket");
        wait.park_permit(wait_cell, &mut self.selected)?;
        let ownership = self.queue.handoff.take().ok_or_else(ownership_fault)?;
        self.complete();
        Ok(ownership)
    }

    fn complete(&mut self) {
        assert!(self.wait.take().is_some(), "live mutex ticket");
        self.queue.retire();
    }
}

impl<T> Drop for Ticket<'_, '_, T> {
    fn drop(&mut self) {
        let Some(wait) = self.wait.take() else {
            return;
        };
        let queued = {
            let mut entries = self.queue.entries.lock();
            match entries.iter().position(|entry| entry.same_cell(wait)) {
                Some(index) => {
                    drop(entries.remove(index).expect("mutex ticket position"));
                    true
                }
                None => false,
            }
        };
        self.queue.retire();
        let selected =
            self.selected || (!queued && wait.take_resource() == Some(ResourceSelection::Permit));
        if !queued && selected {
            let ownership = self.queue.handoff.take().expect("selected mutex owner");
            self.queue.release_ownership(self.value, ownership);
        }
    }
}

fn ownership_fault() -> Error {
    Error::fault(
        crate::error::FaultComponent::Scheduler,
        "resumed mutex ticket does not own the handoff",
    )
}

#[cfg(test)]
#[path = "mutex_queue_test.rs"]
mod mutex_queue_test;
