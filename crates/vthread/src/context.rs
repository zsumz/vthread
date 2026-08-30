//! Carrier-local identity installed while one virtual thread is mounted.

use std::{cell::RefCell, rc::Rc};

use crate::{TaskId, wait::WaitHub};

#[derive(Clone)]
pub(crate) struct MountedTask {
    task: TaskId,
    hub: Rc<WaitHub>,
}

impl MountedTask {
    pub(crate) fn task_id(&self) -> TaskId {
        self.task
    }

    pub(crate) fn hub(&self) -> Rc<WaitHub> {
        Rc::clone(&self.hub)
    }
}

thread_local! {
    static CURRENT: RefCell<Option<MountedTask>> = const { RefCell::new(None) };
}

pub(crate) fn current() -> Option<MountedTask> {
    CURRENT.with(|current| current.borrow().clone())
}

pub(crate) fn mount(task: TaskId, hub: Rc<WaitHub>) -> MountGuard {
    let mounted = MountedTask { task, hub };
    let previous = CURRENT.with(|current| current.replace(Some(mounted)));
    MountGuard { previous }
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
