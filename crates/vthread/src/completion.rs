//! Bounded completion subscriptions; publication follows physical stack reclamation.

use crate::{Error, Result, signal::lock, task::SharedTaskRecord, wait::WaitCell};
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Default)]
struct State {
    done: bool,
    waiters: BTreeMap<u64, WaitCell>,
}

#[cfg(test)]
type CompletionHook = Box<dyn FnOnce(usize) + Send>;

pub(crate) struct Completion {
    capacity: usize,
    state: Mutex<State>,
    #[cfg(test)]
    pub(crate) after_notify: Mutex<Option<CompletionHook>>,
}

impl Completion {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            state: Mutex::default(),
            #[cfg(test)]
            after_notify: Mutex::new(None),
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
            task,
            id: wait.identity(),
        })
    }
}

pub(crate) struct CompletionWait {
    task: SharedTaskRecord,
    id: u64,
}
impl Drop for CompletionWait {
    fn drop(&mut self) {
        lock(&self.task.completion().state).waiters.remove(&self.id);
    }
}

#[cfg(test)]
#[path = "completion_test.rs"]
mod completion_test;
