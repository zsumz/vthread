//! Separate carrier-local storage for owned fibers and borrowed leases.

use crate::{
    context::Execution,
    task_fiber::{BorrowedFiber, OwnedFiber, TakenFiber},
    task_slab::{TaskKey, TaskSlab},
};
use std::rc::Rc;

pub(crate) struct OwnedTask {
    pub(crate) fiber: Option<OwnedFiber>,
    pub(crate) execution: Option<Rc<Execution>>,
}

pub(crate) struct BorrowedTask {
    pub(crate) fiber: Option<BorrowedFiber>,
    pub(crate) execution: Option<Rc<Execution>>,
}

#[derive(Clone, Copy)]
pub(crate) enum TaskRef<'a> {
    Owned(&'a OwnedTask),
    Borrowed(&'a BorrowedTask),
}

impl<'a> TaskRef<'a> {
    pub(crate) fn execution(self) -> &'a Rc<Execution> {
        match self {
            Self::Owned(task) => task.execution.as_ref().expect("task execution"),
            Self::Borrowed(task) => task.execution.as_ref().expect("task execution"),
        }
    }

    pub(crate) fn revoked(self) -> bool {
        match self {
            Self::Owned(_) => false,
            Self::Borrowed(task) => task.fiber.as_ref().is_some_and(BorrowedFiber::revoked),
        }
    }

    pub(crate) fn is_borrowed(self) -> bool {
        matches!(self, Self::Borrowed(_))
    }

    #[cfg(test)]
    pub(crate) fn address(self) -> *const () {
        match self {
            Self::Owned(task) => std::ptr::from_ref(task).cast(),
            Self::Borrowed(task) => std::ptr::from_ref(task).cast(),
        }
    }
}

pub(crate) enum TaskMut<'a> {
    Owned(&'a mut OwnedTask),
    Borrowed(&'a mut BorrowedTask),
}

impl TaskMut<'_> {
    pub(crate) fn execution(&self) -> &Rc<Execution> {
        match self {
            Self::Owned(task) => task.execution.as_ref().expect("task execution"),
            Self::Borrowed(task) => task.execution.as_ref().expect("task execution"),
        }
    }

    pub(crate) fn take_fiber(&mut self) -> Option<TakenFiber> {
        match self {
            Self::Owned(task) => task.fiber.take().map(TakenFiber::Owned),
            Self::Borrowed(task) => task.fiber.take().map(TakenFiber::Borrowed),
        }
    }

    pub(crate) fn take_execution(&mut self) -> Option<Rc<Execution>> {
        match self {
            Self::Owned(task) => task.execution.take(),
            Self::Borrowed(task) => task.execution.take(),
        }
    }
}

pub(crate) struct KernelTasks {
    owned: TaskSlab<OwnedTask>,
    borrowed: TaskSlab<BorrowedTask>,
}

impl KernelTasks {
    pub(crate) const fn new() -> Self {
        Self {
            owned: TaskSlab::new(),
            borrowed: TaskSlab::new(),
        }
    }

    pub(crate) fn insert_owned(&mut self, task: OwnedTask) -> TaskKey {
        let index = self.owned.insert(task);
        let key = TaskKey::owned(index);
        self.owned
            .get(index)
            .expect("inserted owned task")
            .execution
            .as_ref()
            .expect("task execution")
            .assign_task_key(key);
        key
    }

    pub(crate) fn insert_borrowed(&mut self, task: BorrowedTask) -> TaskKey {
        let index = self.borrowed.insert(task);
        let key = TaskKey::borrowed(index);
        self.borrowed
            .get(index)
            .expect("inserted borrowed task")
            .execution
            .as_ref()
            .expect("task execution")
            .assign_task_key(key);
        key
    }

    pub(crate) fn get(&self, key: TaskKey) -> Option<TaskRef<'_>> {
        if key.is_borrowed() {
            self.borrowed.get(key.index()).map(TaskRef::Borrowed)
        } else {
            self.owned.get(key.index()).map(TaskRef::Owned)
        }
    }

    pub(crate) fn get_mut(&mut self, key: TaskKey) -> Option<TaskMut<'_>> {
        if key.is_borrowed() {
            self.borrowed.get_mut(key.index()).map(TaskMut::Borrowed)
        } else {
            self.owned.get_mut(key.index()).map(TaskMut::Owned)
        }
    }

    pub(crate) fn remove(&mut self, key: TaskKey) -> bool {
        if key.is_borrowed() {
            self.borrowed.remove(key.index()).is_some()
        } else {
            self.owned.remove(key.index()).is_some()
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.owned.len() + self.borrowed.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.owned.is_empty() && self.borrowed.is_empty()
    }
}

#[cfg(test)]
#[path = "kernel_tasks_test.rs"]
mod kernel_tasks_test;
