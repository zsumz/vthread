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

    pub(crate) fn enter_after_check(reason: SuspensionReason) -> Result<Self> {
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

    pub(crate) fn park_notification(&self) -> Result<()> {
        crate::parking::park_wait_after_checkpoint::<true, false>(
            self.execution.attached_synchronization_wait(),
            &self.execution,
        )?;
        self.execution.data.check()
    }

    pub(crate) fn parker(&self) -> Result<Parker> {
        self.execution.synchronization_parker()
    }

    pub(crate) fn synchronization_wait(&self) -> Result<&crate::wait::WaitCell> {
        self.execution.synchronization_wait()
    }

    pub(crate) fn attached_synchronization_wait(&self) -> &crate::wait::WaitCell {
        self.execution.attached_synchronization_wait()
    }

    pub(crate) fn park_permit(
        &self,
        wait: &crate::wait::WaitCell,
        selected: &mut bool,
    ) -> Result<()> {
        crate::parking::park_wait_after_checkpoint::<false, true>(wait, &self.execution)?;
        *selected = true;
        self.execution.data.check()
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
