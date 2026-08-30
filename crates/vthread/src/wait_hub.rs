//! Bounded owner-carrier wake inbox: one reserved slot per active generation.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, Weak},
};

use vthread_stack::ParkToken;

use crate::{
    Error, Result,
    signal::{Signal, lock},
    wait::{WaitRegistration, WaitState, WakeNotice},
};

struct Slot {
    state: Weak<Mutex<WaitState>>,
    selected: bool,
}

#[derive(Default)]
struct HubState {
    slots: BTreeMap<ParkToken, Slot>,
    ready: VecDeque<WakeNotice>,
    stale: u64,
}

pub(crate) struct WaitHub {
    capacity: usize,
    state: Mutex<HubState>,
    signal: Arc<Signal>,
}

impl WaitHub {
    pub(crate) fn new(capacity: usize, signal: Arc<Signal>) -> Self {
        Self {
            capacity,
            state: Mutex::default(),
            signal,
        }
    }

    pub(crate) fn register(&self, token: ParkToken, state: Weak<Mutex<WaitState>>) -> Result<()> {
        let mut hub = lock(&self.state);
        if hub.slots.contains_key(&token) {
            return Err(Error::Invariant("wait token registered twice"));
        }
        if hub.slots.len() >= self.capacity {
            return Err(Error::AtCapacity {
                limit: self.capacity,
            });
        }
        hub.slots.insert(
            token,
            Slot {
                state,
                selected: false,
            },
        );
        Ok(())
    }

    pub(crate) fn unregister(&self, token: ParkToken) {
        let mut hub = lock(&self.state);
        hub.slots.remove(&token);
        hub.ready.retain(|notice| notice.token != token);
    }

    pub(crate) fn take_registration(&self, token: ParkToken) -> Result<WaitRegistration> {
        let hub = lock(&self.state);
        let slot = hub
            .slots
            .get(&token)
            .ok_or(Error::Invariant("park request has no wait registration"))?;
        Ok(WaitRegistration {
            state: slot.state.clone(),
        })
    }

    pub(crate) fn enqueue(&self, notice: WakeNotice) {
        let mut hub = lock(&self.state);
        let Some(slot) = hub.slots.get_mut(&notice.token) else {
            hub.stale += 1;
            return;
        };
        if slot.selected {
            hub.stale += 1;
            return;
        }
        slot.selected = true;
        // Each queued notice owns a distinct reserved slot, so ready <= slots <= capacity.
        hub.ready.push_back(notice);
        drop(hub);
        self.signal.notify();
    }

    pub(crate) fn pop_wake(&self) -> Option<WakeNotice> {
        let mut hub = lock(&self.state);
        let notice = hub.ready.pop_front()?;
        hub.slots.remove(&notice.token);
        Some(notice)
    }

    pub(crate) fn pending(&self) -> usize {
        lock(&self.state).ready.len()
    }

    pub(crate) fn stale(&self) -> u64 {
        lock(&self.state).stale
    }
}

#[cfg(test)]
#[path = "wait_hub_test.rs"]
mod wait_hub_test;
