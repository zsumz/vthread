//! Per-virtual-thread state, separate from the native carrier's thread-local storage.

use crate::{Error, Result, SuspensionReason, context, options::TaskOptions};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

pub(crate) struct TaskContext {
    pub(crate) options: TaskOptions,
    pub(crate) reason: Cell<SuspensionReason>,
    pub(crate) masked: Cell<usize>,
    pub(crate) closing: Cell<bool>,
    capacity: usize,
    values: RefCell<BTreeMap<usize, Rc<dyn Any>>>,
}

impl TaskContext {
    pub(crate) fn new(options: TaskOptions, capacity: usize) -> Self {
        Self {
            options,
            capacity,
            reason: Cell::new(SuspensionReason::Park),
            masked: Cell::new(0),
            closing: Cell::new(false),
            values: RefCell::new(BTreeMap::new()),
        }
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.closing.get() {
            return Err(Error::RuntimeStopped);
        }
        if self.masked.get() == 0 {
            self.options.check()?;
        }
        Ok(())
    }

    fn clear(&self) -> Option<crate::PanicReport> {
        let values = self.values.take();
        let mut panic = None;
        for value in values.into_values() {
            if let Err(payload) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(value)))
            {
                let captured = crate::PanicReport::capture(payload);
                panic.get_or_insert(captured);
            }
        }
        panic
    }
}

pub(crate) struct TaskCleanup {
    execution: context::Execution,
    _mounted: context::MountGuard,
}

impl TaskCleanup {
    pub(crate) fn new(
        execution: context::Execution,
        hub: std::sync::Arc<crate::wait::WaitHub>,
    ) -> Self {
        execution.data.closing.set(true);
        let id = crate::signal::lock(&execution.record).id;
        let mounted = context::mount_execution(id, hub, execution.clone());
        Self {
            execution,
            _mounted: mounted,
        }
    }
}

impl Drop for TaskCleanup {
    fn drop(&mut self) {
        if let Some(panic) = self.execution.data.clear() {
            crate::signal::lock(&self.execution.record)
                .panic
                .get_or_insert(panic);
        }
    }
}

/// A typed, lazily initialized virtual-thread-local value.
///
/// Declare as a static key. Each task, including a local child, gets its own value.
pub struct TaskLocal<T: 'static> {
    initialize: fn() -> T,
}

impl<T> TaskLocal<T> {
    /// Creates a key with an initializer evaluated separately in each task.
    pub const fn new(initialize: fn() -> T) -> Self {
        Self { initialize }
    }

    /// Runs a callback with this task's value. The callback may suspend.
    pub fn with<R>(&'static self, body: impl FnOnce(&T) -> R) -> Result<R> {
        let current = context::current().ok_or(Error::OutsideVThread)?;
        let execution = current.execution()?;
        if execution.data.closing.get() {
            return Err(Error::RuntimeStopped);
        }
        let key = std::ptr::from_ref(self) as usize;
        let existing = execution.data.values.borrow().get(&key).cloned();
        let value = if let Some(value) = existing {
            value
        } else {
            if execution.data.values.borrow().len() >= execution.data.capacity {
                return Err(Error::TaskLocalCapacity);
            }
            let value: Rc<dyn Any> = Rc::new((self.initialize)());
            if execution.data.values.borrow().len() >= execution.data.capacity {
                return Err(Error::TaskLocalCapacity);
            }
            let replaced = execution
                .data
                .values
                .borrow_mut()
                .insert(key, Rc::clone(&value));
            drop(replaced);
            value
        };
        Ok(body(
            value.downcast_ref::<T>().expect("typed task-local key"),
        ))
    }
}

#[cfg(test)]
#[path = "task_context_test.rs"]
mod task_context_test;
