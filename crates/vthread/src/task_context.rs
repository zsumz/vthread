//! Per-virtual-thread state, separate from the native carrier's thread-local storage.

#[path = "task_context_reuse.rs"]
mod task_context_reuse;
pub(crate) use task_context_reuse::TaskCleanup;

use crate::{Error, Result, SuspensionReason, context, options::TaskOptions};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

const POLICY_CLOSING: u8 = 1 << 0;
const POLICY_CANCELLED: u8 = 1 << 1;
const POLICY_DEADLINE: u8 = 1 << 2;

pub(crate) struct TaskContext {
    policy: TaskPolicy,
    cold: Box<TaskCold>,
}

pub(crate) struct TaskPolicy {
    cancellation: crate::CancellationToken,
    cancellation_epoch: Arc<AtomicU64>,
    observed_epoch: Cell<u64>,
    masked: Cell<usize>,
    state: Cell<u8>,
}

struct TaskCold {
    deadline: Option<std::time::Instant>,
    reason: Cell<SuspensionReason>,
    capacity: usize,
    values: RefCell<BTreeMap<usize, Value>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CheckpointDecision {
    Continue,
    RuntimeStopped,
    Cancelled,
    DeadlineExceeded,
    CheckDeadline,
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
            self.context.cold.values.borrow_mut().remove(&self.key);
        }
    }
}

impl TaskContext {
    pub(crate) fn new(options: TaskOptions, capacity: usize) -> Self {
        let deadline = options.deadline;
        Self {
            policy: TaskPolicy::new(options.cancellation, deadline.is_some()),
            cold: Box::new(TaskCold {
                deadline,
                capacity,
                reason: Cell::new(SuspensionReason::Park),
                values: RefCell::new(BTreeMap::new()),
            }),
        }
    }

    #[inline]
    pub(crate) fn check(&self) -> Result<()> {
        match self.checkpoint_decision() {
            CheckpointDecision::Continue => Ok(()),
            CheckpointDecision::RuntimeStopped => Err(Error::RuntimeStopped),
            CheckpointDecision::Cancelled => Err(Error::Cancelled),
            CheckpointDecision::DeadlineExceeded => Err(Error::DeadlineExceeded),
            CheckpointDecision::CheckDeadline => unreachable!("resolved deadline decision"),
        }
    }

    #[inline]
    pub(crate) fn interrupted(&self) -> bool {
        self.checkpoint_decision() != CheckpointDecision::Continue
    }

    pub(crate) fn options(&self) -> TaskOptions {
        TaskOptions {
            cancellation: self.policy.cancellation.clone(),
            deadline: self.cold.deadline,
        }
    }

    pub(crate) fn cancellation(&self) -> &crate::CancellationToken {
        &self.policy.cancellation
    }

    pub(crate) fn deadline(&self) -> Option<std::time::Instant> {
        self.cold.deadline
    }

    pub(crate) fn reason(&self) -> SuspensionReason {
        self.cold.reason.get()
    }

    pub(crate) fn replace_reason(&self, reason: SuspensionReason) -> SuspensionReason {
        self.cold.reason.replace(reason)
    }

    pub(crate) fn masked(&self) -> usize {
        self.policy.masked.get()
    }

    pub(crate) fn set_masked(&self, masked: usize) {
        self.policy.masked.set(masked);
    }

    pub(crate) fn close(&self) {
        self.policy
            .state
            .set(self.policy.state.get() | POLICY_CLOSING);
    }

    pub(crate) fn closing(&self) -> bool {
        self.policy.state.get() & POLICY_CLOSING != 0
    }

    fn clear(&self) -> Option<crate::PanicReport> {
        let values = self.cold.values.take();
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

    #[inline]
    fn checkpoint_decision(&self) -> CheckpointDecision {
        match self.policy.checkpoint_decision() {
            CheckpointDecision::CheckDeadline => {
                if self
                    .cold
                    .deadline
                    .is_some_and(|deadline| deadline <= std::time::Instant::now())
                {
                    CheckpointDecision::DeadlineExceeded
                } else {
                    CheckpointDecision::Continue
                }
            }
            decision => decision,
        }
    }
}

impl TaskPolicy {
    fn new(cancellation: crate::CancellationToken, has_deadline: bool) -> Self {
        let (cancellation, cancellation_epoch) = cancellation.into_cancellation_probe();
        Self {
            cancellation,
            cancellation_epoch,
            observed_epoch: Cell::new(0),
            masked: Cell::new(0),
            state: Cell::new(if has_deadline { POLICY_DEADLINE } else { 0 }),
        }
    }

    #[inline]
    fn checkpoint_decision(&self) -> CheckpointDecision {
        let state = self.state.get();
        if state & POLICY_CLOSING != 0 {
            return CheckpointDecision::RuntimeStopped;
        }
        if self.masked.get() != 0 {
            return CheckpointDecision::Continue;
        }
        if state & POLICY_CANCELLED != 0 {
            return CheckpointDecision::Cancelled;
        }
        let epoch = self.cancellation_epoch.load(Ordering::Acquire);
        if epoch != self.observed_epoch.get() {
            self.observed_epoch.set(epoch);
            if self.cancellation.is_cancelled() {
                self.state.set(state | POLICY_CANCELLED);
                return CheckpointDecision::Cancelled;
            }
        }
        if state & POLICY_DEADLINE != 0 {
            return CheckpointDecision::CheckDeadline;
        }
        CheckpointDecision::Continue
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
        if execution.data.closing() {
            return Err(Error::RuntimeStopped);
        }
        let key = std::ptr::from_ref(self) as usize;
        let existing = execution.data.cold.values.borrow().get(&key).cloned();
        let value = match existing {
            Some(Value::Ready(value)) => value,
            Some(Value::Initializing) => return Err(Error::RecursiveTaskLocal),
            None => {
                if execution.data.cold.values.borrow().len() >= execution.data.cold.capacity {
                    return Err(Error::Capacity {
                        resource: crate::error::CapacityResource::TaskLocals,
                        limit: execution.data.cold.capacity,
                    });
                }
                execution
                    .data
                    .cold
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
                    .cold
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
