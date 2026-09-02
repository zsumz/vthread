//! Carrier-local identity installed while one virtual thread is mounted.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use crate::{
    Error, Result, control::Shared, local_carrier::LocalCarrier, task::SharedTaskRecord,
    task_context::TaskContext,
};
use crate::{TaskId, wait::WaitHub};

pub(crate) struct Execution {
    pub(crate) id: TaskId,
    pub(crate) scope: u64,
    pub(crate) hub: Arc<WaitHub>,
    pub(crate) record: SharedTaskRecord,
    pub(crate) shared: Arc<Shared>,
    pub(crate) local: Rc<LocalCarrier>,
    pub(crate) data: Rc<TaskContext>,
    pub(crate) progress: crate::task_progress::TaskProgressWriter,
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
            Self::Execution(execution) => Arc::clone(&execution.hub),
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
        .options
        .cancellation
        .clone())
}

/// Returns the current task's earliest inherited deadline.
pub fn deadline() -> Result<Option<std::time::Instant>> {
    Ok(current()
        .ok_or(Error::OutsideVThread)?
        .execution()?
        .data
        .options
        .deadline)
}

pub(crate) struct MountGuard {
    previous: Option<MountedTask>,
}

impl Drop for MountGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|current| {
            drop(current.replace(previous));
        });
    }
}

#[cfg(test)]
#[path = "context_test.rs"]
mod context_test;
