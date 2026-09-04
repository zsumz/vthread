//! Scoped diagnostic reasons for virtual synchronization waits.

use crate::{Error, Parker, Result, SuspensionReason, context};
use std::rc::Rc;

pub(crate) struct Wait {
    execution: Rc<context::Execution>,
    previous: SuspensionReason,
}

impl Wait {
    pub(crate) fn enter(reason: SuspensionReason) -> Result<Self> {
        context::check_current()?;
        Self::enter_after_check(reason)
    }

    pub(super) fn enter_after_check(reason: SuspensionReason) -> Result<Self> {
        let execution = match context::current().ok_or(Error::OutsideVThread)? {
            context::MountedTask::Execution(execution) => execution,
            context::MountedTask::Cleanup { .. } => return Err(Error::OutsideVThread),
        };
        Ok(Self {
            previous: execution.data.replace_reason(reason),
            execution,
        })
    }

    pub(crate) fn park(&self, parker: &Parker) -> Result<()> {
        parker.park_after_checkpoint(&self.execution)?;
        self.execution.data.check()
    }

    pub(crate) fn parker(&self) -> Result<Parker> {
        self.execution.synchronization_parker()
    }
}

impl Drop for Wait {
    fn drop(&mut self) {
        self.execution.data.replace_reason(self.previous);
    }
}

#[cfg(test)]
#[path = "wait_test.rs"]
mod wait_test;
