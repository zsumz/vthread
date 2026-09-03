//! FIFO permit selection with bounded, cancellation-safe outstanding tickets.

use super::wait::Wait;
use crate::{Error, Parker, Result, SuspensionReason, signal::lock, wait::WaitCell};
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

struct Entry {
    wait: WaitCell,
    // Some(true) returns a selected permit when abandoned; broadcast wakes do not.
    granted: Option<bool>,
}

struct State {
    entries: VecDeque<Entry>,
    vacant: Vec<WaitCell>,
}

pub(super) struct Gate {
    maximum: usize,
    capacity: usize,
    available: AtomicUsize,
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
            closed: AtomicBool::new(false),
            state: Mutex::new(State {
                entries: VecDeque::new(),
                vacant: Vec::new(),
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
        self.subscribe()?.wait(&wait)
    }

    // Registration is also used by condvars before releasing their predicate mutex.
    pub(super) fn subscribe(&self) -> Result<Ticket<'_>> {
        let mut state = lock(&self.state);
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::Closed);
        }
        if state.entries.len() == self.capacity {
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::Waiters,
                limit: self.capacity,
            });
        }
        let parker = Parker {
            wait: state.vacant.pop().unwrap_or_default(),
        };
        let granted = if self.try_take().is_ok() {
            parker.wait.notify();
            Some(true)
        } else {
            None
        };
        state.entries.push_back(Entry {
            wait: parker.wait.clone(),
            granted,
        });
        Ok(Ticket {
            gate: self,
            parker: Some(parker),
        })
    }

    pub(super) fn signal(&self) {
        let wake = {
            let mut state = lock(&self.state);
            if self.closed.load(Ordering::Acquire) {
                return;
            }
            self.return_permit(&mut state)
        };
        if let Some(wait) = wake {
            wait.notify();
        }
    }

    fn return_permit(&self, state: &mut State) -> Option<WaitCell> {
        if let Some(entry) = state
            .entries
            .iter_mut()
            .find(|entry| entry.granted.is_none())
        {
            entry.granted = Some(true);
            return Some(entry.wait.clone());
        }
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
        None
    }

    pub(super) fn broadcast(&self) {
        let wakes = {
            let mut state = lock(&self.state);
            state
                .entries
                .iter_mut()
                .filter_map(|entry| {
                    if entry.granted.is_some() {
                        return None;
                    }
                    entry.granted = Some(false);
                    Some(entry.wait.clone())
                })
                .collect::<Vec<_>>()
        };
        for wait in wakes {
            wait.notify();
        }
    }

    pub(super) fn close(&self) {
        let wakes = {
            let state = lock(&self.state);
            self.closed.store(true, Ordering::Release);
            self.available.store(0, Ordering::Release);
            state
                .entries
                .iter()
                .map(|entry| entry.wait.clone())
                .collect::<Vec<_>>()
        };
        for wait in wakes {
            wait.close();
        }
    }

    pub(super) fn available(&self) -> usize {
        self.available.load(Ordering::Acquire)
    }
    pub(super) fn waiting(&self) -> usize {
        lock(&self.state).entries.len()
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
            let mut state = lock(&gate.state);
            if gate.closed.load(Ordering::Acquire) {
                return Err(Error::Closed);
            }
            let index = state
                .entries
                .iter()
                .position(|entry| entry.wait.same_cell(&self.parker().wait))
                .ok_or(Error::fault(
                    crate::error::FaultComponent::Scheduler,
                    "missing synchronization ticket",
                ))?;
            if state.entries[index].granted.is_some() {
                drop(state.entries.remove(index));
                self.recycle(&mut state);
                return Ok(());
            }
        }
    }

    fn parker(&self) -> &Parker {
        self.parker.as_ref().expect("live synchronization ticket")
    }

    fn recycle(&mut self, state: &mut State) {
        let parker = self.parker.take().expect("live synchronization ticket");
        if parker.wait.recycle() {
            state.vacant.push(parker.wait);
        }
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        let Some(parker) = self.parker.take() else {
            return;
        };
        let wake = {
            let mut state = lock(&self.gate.state);
            let Some(index) = state
                .entries
                .iter()
                .position(|entry| entry.wait.same_cell(&parker.wait))
            else {
                return;
            };
            let entry = state.entries.remove(index).expect("ticket position");
            let granted = entry.granted;
            drop(entry);
            if parker.wait.recycle() {
                state.vacant.push(parker.wait);
            }
            if granted == Some(true) && !self.gate.closed.load(Ordering::Acquire) {
                self.gate.return_permit(&mut state)
            } else {
                None
            }
        };
        if let Some(wait) = wake {
            wait.notify();
        }
    }
}

#[cfg(test)]
#[path = "gate_test.rs"]
mod gate_test;
