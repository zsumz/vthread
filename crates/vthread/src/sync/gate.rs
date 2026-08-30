//! FIFO permit selection with bounded, cancellation-safe outstanding tickets.

use super::wait::Wait;
use crate::{Error, Parker, Result, SuspensionReason, signal::lock, wait::WaitCell};
use std::{collections::VecDeque, sync::Mutex};

struct Entry {
    wait: WaitCell,
    // Some(true) returns a selected permit when abandoned; broadcast wakes do not.
    granted: Option<bool>,
}

struct State {
    available: usize,
    closed: bool,
    entries: VecDeque<Entry>,
}

pub(super) struct Gate {
    maximum: usize,
    capacity: usize,
    state: Mutex<State>,
}

pub(super) struct Ticket<'a> {
    gate: &'a Gate,
    parker: Parker,
}

impl Gate {
    pub(super) fn new(available: usize, maximum: usize, capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(Error::invalid_configuration(
                "wait_capacity",
                "must be positive",
            ));
        }
        Ok(Self {
            maximum,
            capacity,
            state: Mutex::new(State {
                available,
                closed: false,
                entries: VecDeque::new(),
            }),
        })
    }

    pub(super) fn try_take(&self) -> Result<()> {
        let mut state = lock(&self.state);
        if state.closed {
            return Err(Error::Closed);
        }
        if state.available == 0 {
            return Err(Error::WouldBlock);
        }
        state.available -= 1;
        Ok(())
    }

    pub(super) fn take(&self, reason: SuspensionReason) -> Result<()> {
        let wait = Wait::enter(reason)?;
        match self.try_take() {
            Err(Error::WouldBlock) => self.subscribe()?.wait(&wait),
            outcome => outcome,
        }
    }

    // Registration is also used by condvars before releasing their predicate mutex.
    pub(super) fn subscribe(&self) -> Result<Ticket<'_>> {
        let parker = Parker {
            wait: WaitCell::new(),
        };
        let mut state = lock(&self.state);
        if state.closed {
            return Err(Error::Closed);
        }
        if state.entries.len() == self.capacity {
            return Err(Error::WaitQueueFull {
                limit: self.capacity,
            });
        }
        let granted = if state.available > 0 {
            state.available -= 1;
            parker.wait.notify();
            Some(true)
        } else {
            None
        };
        state.entries.push_back(Entry {
            wait: parker.wait.clone(),
            granted,
        });
        Ok(Ticket { gate: self, parker })
    }

    pub(super) fn signal(&self) {
        let wake = {
            let mut state = lock(&self.state);
            if state.closed {
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
        if state.available < self.maximum {
            state.available += 1;
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
            let mut state = lock(&self.state);
            state.closed = true;
            state.available = 0;
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
        lock(&self.state).available
    }
    pub(super) fn waiting(&self) -> usize {
        lock(&self.state).entries.len()
    }
    pub(super) fn is_closed(&self) -> bool {
        lock(&self.state).closed
    }
}

impl Ticket<'_> {
    pub(super) fn wait(self, wait: &Wait) -> Result<()> {
        loop {
            wait.park(&self.parker)?;
            let mut state = lock(&self.gate.state);
            if state.closed {
                return Err(Error::Closed);
            }
            let index = state
                .entries
                .iter()
                .position(|entry| entry.wait.identity() == self.parker.wait.identity())
                .ok_or(Error::Invariant("missing synchronization ticket"))?;
            if state.entries[index].granted.is_some() {
                state.entries.remove(index);
                return Ok(());
            }
        }
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        let wake = {
            let mut state = lock(&self.gate.state);
            let Some(index) = state
                .entries
                .iter()
                .position(|entry| entry.wait.identity() == self.parker.wait.identity())
            else {
                return;
            };
            let entry = state.entries.remove(index).expect("ticket position");
            if entry.granted == Some(true) && !state.closed {
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
