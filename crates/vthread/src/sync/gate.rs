//! FIFO permit selection with bounded, cancellation-safe outstanding tickets.

use super::wait::Wait;
use crate::{
    Error, Parker, Result, SuspensionReason,
    signal::lock,
    wait::{ResourceSelection, WaitCell},
};
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

struct Entry {
    wait: WaitCell,
}

struct State {
    entries: VecDeque<Entry>,
}

pub(super) struct Gate {
    maximum: usize,
    capacity: usize,
    available: AtomicUsize,
    outstanding: AtomicUsize,
    closed: AtomicBool,
    state: Mutex<State>,
}

pub(super) struct Ticket<'a> {
    gate: &'a Gate,
    parker: Option<Parker>,
}

impl Gate {
    pub(super) fn new(available: usize, maximum: usize, capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::WaitCapacity,
                "must be positive",
            ));
        }
        Ok(Self {
            maximum,
            capacity,
            available: AtomicUsize::new(available),
            outstanding: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            state: Mutex::new(State {
                entries: VecDeque::new(),
            }),
        })
    }

    pub(super) fn try_take(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::Closed);
        }
        let mut available = self.available.load(Ordering::Acquire);
        loop {
            if available == 0 {
                return if self.closed.load(Ordering::Acquire) {
                    Err(Error::Closed)
                } else {
                    Err(Error::WouldBlock)
                };
            }
            match self.available.compare_exchange_weak(
                available,
                available - 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(observed) => available = observed,
            }
        }
    }

    pub(super) fn take(&self, reason: SuspensionReason) -> Result<()> {
        let wait = Wait::enter(reason)?;
        match self.try_take() {
            Err(Error::WouldBlock) => {}
            outcome => return outcome,
        }
        self.subscribe(&wait)?.wait(&wait)
    }

    // Registration is also used by condvars before releasing their predicate mutex.
    pub(super) fn subscribe(&self, wait: &Wait) -> Result<Ticket<'_>> {
        let parker = wait.parker()?;
        self.subscribe_parker(parker)
    }

    #[cfg(test)]
    pub(super) fn subscribe_test(&self) -> Result<Ticket<'_>> {
        self.subscribe_parker(Parker {
            wait: WaitCell::new(),
        })
    }

    fn subscribe_parker(&self, parker: Parker) -> Result<Ticket<'_>> {
        self.reserve()?;
        let mut state = lock(&self.state);
        if self.closed.load(Ordering::Acquire) {
            self.release();
            return Err(Error::Closed);
        }
        if self.try_take().is_ok() {
            if !parker.wait.offer_resource(ResourceSelection::Permit) {
                self.store_permit();
                self.release();
                return Err(Error::fault(
                    crate::error::FaultComponent::Scheduler,
                    "synchronization wait retained a prior selection",
                ));
            }
        } else {
            state.entries.push_back(Entry {
                wait: parker.wait.clone(),
            });
        }
        Ok(Ticket {
            gate: self,
            parker: Some(parker),
        })
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

    fn release(&self) {
        let previous = self.outstanding.fetch_sub(1, Ordering::AcqRel);
        assert!(previous != 0, "synchronization ticket released twice");
    }

    pub(super) fn signal(&self) {
        loop {
            let mut state = lock(&self.state);
            if self.closed.load(Ordering::Acquire) {
                return;
            }
            let Some(entry) = state.entries.front() else {
                self.store_permit();
                return;
            };
            let selected = entry.wait.offer_resource(ResourceSelection::Permit);
            drop(
                state
                    .entries
                    .pop_front()
                    .expect("front synchronization wait"),
            );
            drop(state);
            if selected {
                return;
            }
        }
    }

    fn store_permit(&self) {
        let mut available = self.available.load(Ordering::Relaxed);
        while available < self.maximum {
            match self.available.compare_exchange_weak(
                available,
                available + 1,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => available = observed,
            }
        }
    }

    pub(super) fn broadcast(&self) {
        let mut state = lock(&self.state);
        for entry in &state.entries {
            let _ = entry.wait.offer_resource(ResourceSelection::Broadcast);
        }
        state.entries.clear();
    }

    pub(super) fn close(&self) {
        let mut state = lock(&self.state);
        self.closed.store(true, Ordering::Release);
        self.available.store(0, Ordering::Release);
        for entry in &state.entries {
            entry.wait.close();
        }
        state.entries.clear();
    }

    pub(super) fn available(&self) -> usize {
        self.available.load(Ordering::Acquire)
    }
    pub(super) fn waiting(&self) -> usize {
        self.outstanding.load(Ordering::Acquire)
    }
    pub(super) fn wait_capacity(&self) -> usize {
        self.capacity
    }
    pub(super) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

impl Ticket<'_> {
    pub(super) fn wait(mut self, wait: &Wait) -> Result<()> {
        let gate = self.gate;
        loop {
            wait.park(self.parker())?;
            if gate.closed.load(Ordering::Acquire) {
                return Err(Error::Closed);
            }
            if self.parker().wait.take_resource().is_some() {
                self.complete();
                return Ok(());
            }
        }
    }

    fn parker(&self) -> &Parker {
        self.parker.as_ref().expect("live synchronization ticket")
    }

    fn complete(&mut self) {
        drop(self.parker.take().expect("live synchronization ticket"));
        self.gate.release();
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        let Some(parker) = self.parker.take() else {
            return;
        };
        let selection = {
            let mut state = lock(&self.gate.state);
            match state
                .entries
                .iter()
                .position(|entry| entry.wait.same_cell(&parker.wait))
            {
                Some(index) => {
                    drop(state.entries.remove(index).expect("ticket position"));
                    None
                }
                None => parker.wait.take_resource(),
            }
        };
        self.gate.release();
        if selection == Some(ResourceSelection::Permit) {
            self.gate.signal();
        }
    }
}

#[cfg(test)]
#[path = "gate_test.rs"]
mod gate_test;
