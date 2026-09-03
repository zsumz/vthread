//! Carrier-local identity installed while one virtual thread is mounted.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use crate::{
    Error, Result, control::Shared, local_carrier::LocalCarrier, task::SharedTaskRecord,
    task_context::TaskContext,
};
use crate::{TaskId, wait::WaitHub};

pub(crate) struct Execution {
    pub(crate) id: TaskId,
    pub(crate) data: Rc<TaskContext>,
    pub(crate) progress: crate::task_progress::TaskProgressWriter,
    cold: Box<ExecutionCold>,
}

struct ExecutionCold {
    scope: u64,
    hub: Arc<WaitHub>,
    record: SharedTaskRecord,
    shared: Arc<Shared>,
    local: Rc<LocalCarrier>,
}

impl Execution {
    pub(crate) fn new(
        id: TaskId,
        scope: u64,
        hub: Arc<WaitHub>,
        record: SharedTaskRecord,
        shared: Arc<Shared>,
        local: Rc<LocalCarrier>,
        data: Rc<TaskContext>,
    ) -> Self {
        Self {
            id,
            data,
            progress: crate::task_progress::TaskProgressWriter::new(),
            cold: Box::new(ExecutionCold {
                scope,
                hub,
                record,
                shared,
                local,
            }),
        }
    }

    pub(crate) fn scope(&self) -> u64 {
        self.cold.scope
    }

    pub(crate) fn hub(&self) -> &Arc<WaitHub> {
        &self.cold.hub
    }

    pub(crate) fn record(&self) -> &SharedTaskRecord {
        &self.cold.record
    }

    pub(crate) fn shared(&self) -> &Arc<Shared> {
        &self.cold.shared
    }

    pub(crate) fn local(&self) -> &Rc<LocalCarrier> {
        &self.cold.local
    }
}

#[derive(Clone)]
pub(crate) enum MountedTask {
    Execution(Rc<Execution>),
    Cleanup { task: TaskId, hub: Arc<WaitHub> },
}

impl MountedTask {
    pub(crate) fn execution(&self) -> Result<&Rc<Execution>> {
        match self {
            Self::Execution(execution) => Ok(execution),
            Self::Cleanup { .. } => Err(Error::OutsideVThread),
        }
    }
    pub(crate) fn task_id(&self) -> TaskId {
        match self {
            Self::Execution(execution) => execution.id,
            Self::Cleanup { task, .. } => *task,
        }
    }

    pub(crate) fn hub(&self) -> Arc<WaitHub> {
        match self {
            Self::Execution(execution) => Arc::clone(execution.hub()),
            Self::Cleanup { hub, .. } => Arc::clone(hub),
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Option<MountedTask>> = const { RefCell::new(None) };
}

pub(crate) fn current() -> Option<MountedTask> {
    CURRENT.with(|current| current.borrow().clone())
}

pub(crate) fn mount(task: TaskId, hub: Arc<WaitHub>) -> MountGuard {
    install(MountedTask::Cleanup { task, hub })
}

pub(crate) fn mount_execution(execution: Rc<Execution>) -> MountGuard {
    install(MountedTask::Execution(execution))
}

pub(crate) fn with_execution_slot<R>(
    slot: &mut Option<Rc<Execution>>,
    body: impl FnOnce(&Rc<Execution>) -> R,
) -> R {
    CURRENT.with(|current| {
        let execution = slot.take().expect("unmounted task execution");
        let previous = current.replace(Some(MountedTask::Execution(execution)));
        let mounted = ExecutionSlotMount {
            current,
            slot,
            previous,
        };
        mounted.with_execution(body)
    })
}

fn install(mounted: MountedTask) -> MountGuard {
    let previous = CURRENT.with(|current| current.replace(Some(mounted)));
    MountGuard { previous }
}

/// Checks inherited cancellation and the earliest deadline at a cooperative boundary.
#[inline]
pub fn checkpoint() -> Result<()> {
    CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .ok_or(Error::OutsideVThread)?
            .execution()?
            .data
            .check()
    })
}

/// Returns the current task's inherited cancellation token.
pub fn cancellation_token() -> Result<crate::CancellationToken> {
    Ok(current()
        .ok_or(Error::OutsideVThread)?
        .execution()?
        .data
        .options()
        .cancellation
        .clone())
}

/// Returns the current task's earliest inherited deadline.
pub fn deadline() -> Result<Option<std::time::Instant>> {
    Ok(current()
        .ok_or(Error::OutsideVThread)?
        .execution()?
        .data
        .options()
        .deadline)
}

pub(crate) struct MountGuard {
    previous: Option<MountedTask>,
}

pub(crate) struct ExecutionSlotMount<'a> {
    current: &'a RefCell<Option<MountedTask>>,
    slot: &'a mut Option<Rc<Execution>>,
    previous: Option<MountedTask>,
}

impl ExecutionSlotMount<'_> {
    pub(crate) fn with_execution<R>(&self, body: impl FnOnce(&Rc<Execution>) -> R) -> R {
        let mounted = self.current.borrow();
        let execution = mounted
            .as_ref()
            .expect("mounted task execution")
            .execution()
            .expect("mounted execution context");
        body(execution)
    }
}

impl Drop for MountGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|current| {
            drop(current.replace(previous));
        });
    }
}

impl Drop for ExecutionSlotMount<'_> {
    fn drop(&mut self) {
        let previous = self.previous.take();
        let mounted = self.current.replace(previous);
        let execution = match mounted {
            Some(MountedTask::Execution(execution)) => execution,
            _ => panic!("task execution mount was replaced"),
        };
        assert!(
            self.slot.replace(execution).is_none(),
            "occupied execution slot"
        );
    }
}

#[cfg(test)]
#[path = "context_test.rs"]
mod context_test;
