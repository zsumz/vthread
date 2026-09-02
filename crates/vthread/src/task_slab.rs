//! Carrier-local task storage behind compact scheduler keys.

use std::num::NonZeroUsize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskKey(NonZeroUsize);

impl TaskKey {
    fn from_index(index: usize) -> Self {
        Self(
            NonZeroUsize::new(index.checked_add(1).expect("task key space exhausted"))
                .expect("encoded task key is nonzero"),
        )
    }

    fn index(self) -> usize {
        self.0.get() - 1
    }
}

pub(crate) struct TaskSlab<T> {
    slots: Vec<Option<T>>,
    vacant: Vec<TaskKey>,
}

impl<T> TaskSlab<T> {
    pub(crate) const fn new() -> Self {
        Self {
            slots: Vec::new(),
            vacant: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, value: T) -> TaskKey {
        let Some(key) = self.vacant.pop() else {
            let key = TaskKey::from_index(self.slots.len());
            self.slots.push(Some(value));
            return key;
        };
        assert!(
            self.slots[key.index()].is_none(),
            "occupied vacant task slot"
        );
        self.slots[key.index()] = Some(value);
        key
    }

    pub(crate) fn get(&self, key: TaskKey) -> Option<&T> {
        self.slots.get(key.index())?.as_ref()
    }

    pub(crate) fn get_mut(&mut self, key: TaskKey) -> Option<&mut T> {
        self.slots.get_mut(key.index())?.as_mut()
    }

    pub(crate) fn remove(&mut self, key: TaskKey) -> Option<T> {
        let value = self.slots.get_mut(key.index())?.take()?;
        self.vacant.push(key);
        Some(value)
    }

    pub(crate) fn len(&self) -> usize {
        self.slots.len() - self.vacant.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
#[path = "task_slab_test.rs"]
mod task_slab_test;
