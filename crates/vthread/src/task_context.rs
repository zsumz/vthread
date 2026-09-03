//! Per-virtual-thread state, separate from the native carrier's thread-local storage.

use crate::{Error, Result, SuspensionReason, context, options::TaskOptions};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) struct TaskContext {
    pub(crate) options: TaskOptions,
    cancellation: Arc<AtomicBool>,
    pub(crate) reason: Cell<SuspensionReason>,
    pub(crate) masked: Cell<usize>,
    pub(crate) closing: Cell<bool>,
    capacity: usize,
    values: RefCell<BTreeMap<usize, Value>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CheckpointDecision {
    Continue,
    RuntimeStopped,
    Cancelled,
    DeadlineExceeded,
}

#[derive(Clone)]
enum Value {
    Initializing,
    Ready(Rc<dyn Any>),
}

struct Initializing<'a> {
    context: &'a TaskContext,
    key: usize,
    pending: bool,
}
impl Drop for Initializing<'_> {
    fn drop(&mut self) {
        if self.pending {
            self.context.values.borrow_mut().remove(&self.key);
        }
    }
}

impl TaskContext {
    pub(crate) fn new(options: TaskOptions, capacity: usize) -> Self {
        let cancellation = options.cancellation.cancellation_flag();
        Self {
            options,
            cancellation,
            capacity,
            reason: Cell::new(SuspensionReason::Park),
            masked: Cell::new(0),
            closing: Cell::new(false),
            values: RefCell::new(BTreeMap::new()),
        }
    }

    pub(crate) fn check(&self) -> Result<()> {
        match self.checkpoint_decision() {
            CheckpointDecision::Continue => Ok(()),
            CheckpointDecision::RuntimeStopped => Err(Error::RuntimeStopped),
            CheckpointDecision::Cancelled => Err(Error::Cancelled),
            CheckpointDecision::DeadlineExceeded => Err(Error::DeadlineExceeded),
        }
    }

    #[inline]
    pub(crate) fn interrupted(&self) -> bool {
        self.checkpoint_decision() != CheckpointDecision::Continue
    }

    #[inline]
    fn checkpoint_decision(&self) -> CheckpointDecision {
        if self.closing.get() {
            return CheckpointDecision::RuntimeStopped;
        }
        if self.masked.get() != 0 {
            return CheckpointDecision::Continue;
        }
        if self.cancellation.load(Ordering::Acquire) {
            return CheckpointDecision::Cancelled;
        }
        if self
            .options
            .deadline
            .is_some_and(|deadline| deadline <= std::time::Instant::now())
        {
            return CheckpointDecision::DeadlineExceeded;
        }
        CheckpointDecision::Continue
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
    execution: Rc<context::Execution>,
    _mounted: context::MountGuard,
}

impl TaskCleanup {
    pub(crate) fn new(execution: Rc<context::Execution>) -> Self {
        execution.data.closing.set(true);
        let mounted = context::mount_execution(Rc::clone(&execution));
        Self {
            execution,
            _mounted: mounted,
        }
    }
}

impl Drop for TaskCleanup {
    fn drop(&mut self) {
        if let Some(panic) = self.execution.data.clear() {
            self.execution.record.lock().panic.get_or_insert(panic);
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
        let value = match existing {
            Some(Value::Ready(value)) => value,
            Some(Value::Initializing) => return Err(Error::RecursiveTaskLocal),
            None => {
                if execution.data.values.borrow().len() >= execution.data.capacity {
                    return Err(Error::Capacity {
                        resource: crate::error::CapacityResource::TaskLocals,
                        limit: execution.data.capacity,
                    });
                }
                execution
                    .data
                    .values
                    .borrow_mut()
                    .insert(key, Value::Initializing);
                let mut initializing = Initializing {
                    context: &execution.data,
                    key,
                    pending: true,
                };
                let value: Rc<dyn Any> = Rc::new((self.initialize)());
                let replaced = execution
                    .data
                    .values
                    .borrow_mut()
                    .insert(key, Value::Ready(Rc::clone(&value)));
                initializing.pending = false;
                drop(replaced);
                value
            }
        };
        Ok(body(
            value.downcast_ref::<T>().expect("typed task-local key"),
        ))
    }
}

#[cfg(test)]
#[path = "task_context_test.rs"]
mod task_context_test;
