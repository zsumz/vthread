//! Carrier-local identity installed while one virtual thread is mounted.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use crate::{
    Error, Result, control::Shared, local_carrier::LocalCarrier, task::SharedTaskRecord,
    task_context::TaskContext,
};
use crate::{TaskId, wait::WaitHub};

#[derive(Clone)]
pub(crate) struct Execution {
    pub(crate) record: SharedTaskRecord,
    pub(crate) shared: Arc<Shared>,
    pub(crate) local: Rc<LocalCarrier>,
    pub(crate) data: Rc<TaskContext>,
}

#[derive(Clone)]
pub(crate) struct MountedTask {
    task: TaskId,
    hub: Arc<WaitHub>,
    execution: Option<Execution>,
}

impl MountedTask {
    pub(crate) fn execution(&self) -> Result<&Execution> {
        self.execution.as_ref().ok_or(Error::OutsideVThread)
    }
    pub(crate) fn task_id(&self) -> TaskId {
        self.task
    }

    pub(crate) fn hub(&self) -> Arc<WaitHub> {
        Arc::clone(&self.hub)
    }
}

thread_local! {
    static CURRENT: RefCell<Option<MountedTask>> = const { RefCell::new(None) };
}

pub(crate) fn current() -> Option<MountedTask> {
    CURRENT.with(|current| current.borrow().clone())
}

pub(crate) fn mount(task: TaskId, hub: Arc<WaitHub>) -> MountGuard {
    install(MountedTask {
        task,
        hub,
        execution: None,
    })
}

pub(crate) fn mount_execution(task: TaskId, hub: Arc<WaitHub>, execution: Execution) -> MountGuard {
    install(MountedTask {
        task,
        hub,
        execution: Some(execution),
    })
}

fn install(mounted: MountedTask) -> MountGuard {
    let previous = CURRENT.with(|current| current.replace(Some(mounted)));
    MountGuard { previous }
}

/// Checks inherited cancellation and the earliest deadline at a cooperative boundary.
pub fn checkpoint() -> Result<()> {
    current()
        .ok_or(Error::OutsideVThread)?
        .execution()?
        .data
        .check()
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
