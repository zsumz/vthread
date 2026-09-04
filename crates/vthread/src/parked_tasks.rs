//! Dense carrier-owned park records addressed by stable task slab keys.

use crate::{task_slab::TaskKey, wait::WaitRegistration};
use vthread_stack::ParkToken;

pub(super) struct ParkedTask {
    pub(super) token: ParkToken,
    pub(super) task: TaskKey,
    pub(super) has_deadline: bool,
    // `None` is valid only for this task's exact resident synchronization generation.
    pub(super) registration: Option<WaitRegistration>,
}

pub(super) struct ParkedTasks {
    owned: Vec<Option<ParkedTask>>,
    borrowed: Vec<Option<ParkedTask>>,
    len: usize,
}

impl ParkedTasks {
    pub(super) const fn new() -> Self {
        Self {
            owned: Vec::new(),
            borrowed: Vec::new(),
            len: 0,
        }
    }

    pub(super) fn insert(&mut self, parked: ParkedTask) -> bool {
        let task = parked.task;
        let slots = self.slots_mut(task);
        if slots.len() <= task.index() {
            slots.resize_with(task.index() + 1, || None);
        }
        let slot = &mut slots[task.index()];
        if slot.is_some() {
            return false;
        }
        *slot = Some(parked);
        self.len += 1;
        true
    }

    pub(super) fn get(&self, task: TaskKey) -> Option<&ParkedTask> {
        self.slots(task).get(task.index())?.as_ref()
    }

    pub(super) fn remove(&mut self, task: TaskKey) -> Option<ParkedTask> {
        let parked = self.slots_mut(task).get_mut(task.index())?.take()?;
        self.len -= 1;
        Some(parked)
    }

    pub(super) fn find_token(&self, token: ParkToken) -> Option<&ParkedTask> {
        self.iter().find(|parked| parked.token == token)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &ParkedTask> {
        self.owned
            .iter()
            .chain(&self.borrowed)
            .filter_map(Option::as_ref)
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn slots(&self, task: TaskKey) -> &[Option<ParkedTask>] {
        if task.is_borrowed() {
            &self.borrowed
        } else {
            &self.owned
        }
    }

    fn slots_mut(&mut self, task: TaskKey) -> &mut Vec<Option<ParkedTask>> {
        if task.is_borrowed() {
            &mut self.borrowed
        } else {
            &mut self.owned
        }
    }
}

#[cfg(test)]
#[path = "parked_tasks_test.rs"]
mod parked_tasks_test;
