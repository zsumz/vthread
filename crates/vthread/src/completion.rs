//! Bounded completion subscriptions; publication follows physical stack reclamation.

use crate::{Error, Result, signal::lock, wait::WaitCell};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct State {
    done: bool,
    waiters: BTreeMap<u64, WaitCell>,
}

pub(crate) struct Completion {
    capacity: usize,
    state: Mutex<State>,
}

impl Completion {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            state: Mutex::default(),
        }
    }
    pub(crate) fn done(&self) -> bool {
        lock(&self.state).done
    }
    pub(crate) fn complete(&self) {
        let waiters = {
            let mut state = lock(&self.state);
            state.done = true;
            std::mem::take(&mut state.waiters)
        };
        for wait in waiters.into_values() {
            wait.notify();
        }
    }

    pub(crate) fn subscribe(self: &Arc<Self>, wait: &WaitCell) -> Result<CompletionWait> {
        let mut state = lock(&self.state);
        if state.done {
            wait.notify();
        } else {
            if state.waiters.len() >= self.capacity {
                return Err(Error::Capacity {
                    resource: crate::error::CapacityResource::Waiters,
                    limit: self.capacity,
                });
            }
            state.waiters.insert(wait.identity(), wait.clone());
        }
        Ok(CompletionWait {
            completion: Arc::clone(self),
            id: wait.identity(),
        })
    }
}

pub(crate) struct CompletionWait {
    completion: Arc<Completion>,
    id: u64,
}
impl Drop for CompletionWait {
    fn drop(&mut self) {
        lock(&self.completion.state).waiters.remove(&self.id);
    }
}

#[cfg(test)]
#[path = "completion_test.rs"]
mod completion_test;
