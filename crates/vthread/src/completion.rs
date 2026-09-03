//! Bounded completion subscriptions; publication follows physical stack reclamation.

use crate::{Error, Result, signal::lock, task::SharedTaskRecord, wait::WaitCell};
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

const DONE: usize = 1;
const WAITER: usize = 2;

#[derive(Default)]
struct State {
    waiters: BTreeMap<u64, WaitCell>,
}

#[cfg(test)]
type CompletionHook = Box<dyn FnOnce(usize) + Send>;

pub(crate) struct Completion {
    capacity: usize,
    lifecycle: AtomicUsize,
    state: Mutex<State>,
    #[cfg(test)]
    pub(crate) after_notify: Mutex<Option<CompletionHook>>,
}

impl Completion {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            lifecycle: AtomicUsize::new(0),
            state: Mutex::default(),
            #[cfg(test)]
            after_notify: Mutex::new(None),
        }
    }
    pub(crate) fn done(&self) -> bool {
        self.lifecycle.load(Ordering::Acquire) & DONE != 0
    }
    pub(crate) fn complete(&self) {
        let registered = self.lifecycle.fetch_or(DONE, Ordering::AcqRel) / WAITER;
        let waiters = if registered == 0 {
            BTreeMap::new()
        } else {
            let mut state = lock(&self.state);
            std::mem::take(&mut state.waiters)
        };
        if !waiters.is_empty() {
            self.lifecycle
                .fetch_sub(waiters.len() * WAITER, Ordering::Release);
        }
        #[cfg(test)]
        let mut selected = 0;
        for wait in waiters.into_values() {
            let _notification = wait.notify();
            #[cfg(test)]
            {
                selected += usize::from(_notification == crate::wait::NotifyResult::Woke);
            }
        }
        #[cfg(test)]
        {
            let hook = lock(&self.after_notify).take();
            if let Some(hook) = hook {
                hook(selected);
            }
        }
    }

    pub(crate) fn subscribe(
        &self,
        task: SharedTaskRecord,
        wait: &WaitCell,
    ) -> Result<CompletionWait> {
        // Reserve before locking. A racing completion either sees this count and
        // drains the map, or publishes DONE for the subscriber to observe.
        let lifecycle = self.lifecycle.fetch_add(WAITER, Ordering::AcqRel);
        if lifecycle & DONE != 0 {
            self.lifecycle.fetch_sub(WAITER, Ordering::Release);
            wait.notify();
            return Ok(CompletionWait {
                task,
                id: wait.identity(),
            });
        }
        let mut state = lock(&self.state);
        if self.done() {
            self.lifecycle.fetch_sub(WAITER, Ordering::Release);
            wait.notify();
        } else {
            if state.waiters.len() >= self.capacity {
                self.lifecycle.fetch_sub(WAITER, Ordering::Release);
                return Err(Error::Capacity {
                    resource: crate::error::CapacityResource::Waiters,
                    limit: self.capacity,
                });
            }
            state.waiters.insert(wait.identity(), wait.clone());
        }
        Ok(CompletionWait {
            task,
            id: wait.identity(),
        })
    }

    pub(crate) fn reset(&mut self) {
        *self.lifecycle.get_mut() = 0;
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        assert!(state.waiters.is_empty(), "recycled completion has waiters");
    }
}

pub(crate) struct CompletionWait {
    task: SharedTaskRecord,
    id: u64,
}
impl Drop for CompletionWait {
    fn drop(&mut self) {
        let completion = self.task.completion();
        if lock(&completion.state).waiters.remove(&self.id).is_some() {
            completion.lifecycle.fetch_sub(WAITER, Ordering::Release);
        }
    }
}

#[cfg(test)]
#[path = "completion_test.rs"]
mod completion_test;
