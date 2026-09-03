//! Carrier-local task storage behind compact scheduler keys.

use std::num::NonZeroUsize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskKey(NonZeroUsize);

impl TaskKey {
    pub(crate) fn owned(index: usize) -> Self {
        Self::encode(index, false)
    }

    pub(crate) fn borrowed(index: usize) -> Self {
        Self::encode(index, true)
    }

    fn encode(index: usize, borrowed: bool) -> Self {
        let slot = index.checked_add(1).expect("task key space exhausted");
        let encoded = slot
            .checked_mul(2)
            .and_then(|value| value.checked_add(usize::from(borrowed)))
            .expect("task key space exhausted");
        Self(NonZeroUsize::new(encoded).expect("encoded task key is nonzero"))
    }

    pub(crate) fn index(self) -> usize {
        (self.0.get() >> 1) - 1
    }

    pub(crate) fn is_borrowed(self) -> bool {
        self.0.get() & 1 != 0
    }
}

pub(crate) struct TaskSlab<T> {
    slots: Vec<Option<T>>,
    vacant: Vec<usize>,
}

impl<T> TaskSlab<T> {
    pub(crate) const fn new() -> Self {
        Self {
            slots: Vec::new(),
            vacant: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, value: T) -> usize {
        let Some(index) = self.vacant.pop() else {
            let index = self.slots.len();
            self.slots.push(Some(value));
            return index;
        };
        assert!(self.slots[index].is_none(), "occupied vacant task slot");
        self.slots[index] = Some(value);
        index
    }

    pub(crate) fn get(&self, index: usize) -> Option<&T> {
        self.slots.get(index)?.as_ref()
    }

    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.slots.get_mut(index)?.as_mut()
    }

    pub(crate) fn remove(&mut self, index: usize) -> Option<T> {
        let value = self.slots.get_mut(index)?.take()?;
        self.vacant.push(index);
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
