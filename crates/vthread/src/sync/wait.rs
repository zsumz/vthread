//! Scoped diagnostic reasons for virtual synchronization waits.

use crate::{Error, Parker, Result, SuspensionReason, context, task_context::TaskContext};
use std::rc::Rc;

pub(crate) struct Wait {
    data: Rc<TaskContext>,
    previous: SuspensionReason,
}

impl Wait {
    pub(crate) fn enter(reason: SuspensionReason) -> Result<Self> {
        let mounted = context::current().ok_or(Error::OutsideVThread)?;
        let data = Rc::clone(&mounted.execution()?.data);
        data.check()?;
        Ok(Self {
            previous: data.reason.replace(reason),
            data,
        })
    }

    pub(crate) fn park(&self, parker: &Parker) -> Result<()> {
        parker.park()?;
        self.data.check()
    }
}

impl Drop for Wait {
    fn drop(&mut self) {
        self.data.reason.set(self.previous);
    }
}

#[cfg(test)]
#[path = "wait_test.rs"]
mod wait_test;
