//! Carrier-local identity installed while one virtual thread is mounted.

#[path = "context_wake.rs"]
mod wake;
pub(crate) use wake::{enqueue_local_wake, unregister_local_wake};
#[path = "context_pending.rs"]
mod pending;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

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
    record: Option<SharedTaskRecord>,
    shared: Arc<Shared>,
    local: Rc<LocalCarrier>,
    pending_wait: RefCell<Option<pending::PendingWait>>,
    readiness_wait: RefCell<Option<crate::wait::WaitCell>>,
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
                record: Some(record),
                shared,
                local,
                pending_wait: RefCell::new(None),
                readiness_wait: RefCell::new(None),
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
        self.cold.record.as_ref().expect("live task record")
    }

    pub(crate) fn shared(&self) -> &Arc<Shared> {
        &self.cold.shared
    }

    pub(crate) fn local(&self) -> &Rc<LocalCarrier> {
        &self.cold.local
    }

    pub(crate) fn readiness_parker(&self) -> Result<crate::Parker> {
        let mut cached = self.cold.readiness_wait.borrow_mut();
        let wait = cached.get_or_insert_with(crate::wait::WaitCell::new);
        if !wait.recycle() {
            return Err(Error::fault(
                crate::error::FaultComponent::Readiness,
                "cached readiness wait remained active",
            ));
        }
        Ok(crate::Parker { wait: wait.clone() })
    }

    pub(crate) fn recycle(&mut self) -> bool {
        let Some(data) = Rc::get_mut(&mut self.data) else {
            return false;
        };
        let cancellation = self.cold.shared.cancellation.clone();
        data.recycle(cancellation);
        drop(self.cold.record.take().expect("live task record"));
        true
    }

    pub(crate) fn reuse(
        &mut self,
        id: TaskId,
        scope: u64,
        record: SharedTaskRecord,
        options: crate::options::TaskOptions,
        task_local_capacity: usize,
    ) {
        assert!(
            self.cold.record.is_none(),
            "execution reused before recycling"
        );
        self.id = id;
        Rc::get_mut(&mut self.data)
            .expect("cached task context must be unique")
            .reuse(options, task_local_capacity);
        self.progress.reset();
        self.cold.scope = scope;
        self.cold.record = Some(record);
    }
}

#[derive(Clone)]
pub(crate) enum MountedTask {
    Execution(Rc<Execution>),
    Cleanup { task: TaskId },
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
            Self::Cleanup { task } => *task,
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Option<MountedTask>> = const { RefCell::new(None) };
    static CARRIER_RUNNABLE: Cell<bool> = const { Cell::new(false) };
}

pub(crate) static MOUNTED_EXECUTION: vthread_stack::ContextKey<Rc<Execution>> =
    vthread_stack::ContextKey::new();

pub(crate) fn current() -> Option<MountedTask> {
    CURRENT
        .with(|current| current.borrow().clone())
        .or_else(|| {
            MOUNTED_EXECUTION.with(|execution| MountedTask::Execution(Rc::clone(execution)))
        })
}

pub(crate) fn mount(task: TaskId) -> MountGuard {
    install(MountedTask::Cleanup { task })
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
    current()
        .ok_or(Error::OutsideVThread)?
        .execution()?
        .data
        .check()
}

pub(crate) fn set_carrier_runnable(runnable: bool) {
    CARRIER_RUNNABLE.set(runnable);
}

#[inline]
pub(crate) fn carrier_has_runnable() -> bool {
    CARRIER_RUNNABLE.get()
}

/// Returns the current task's inherited cancellation token.
pub fn cancellation_token() -> Result<crate::CancellationToken> {
    Ok(current()
        .ok_or(Error::OutsideVThread)?
        .execution()?
        .data
        .cancellation()
        .clone())
}

/// Returns the current task's earliest inherited deadline.
pub fn deadline() -> Result<Option<std::time::Instant>> {
    Ok(current()
        .ok_or(Error::OutsideVThread)?
        .execution()?
        .data
        .deadline())
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
