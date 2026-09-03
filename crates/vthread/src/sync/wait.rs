//! Scoped diagnostic reasons for virtual synchronization waits.

use crate::{Error, Parker, Result, SuspensionReason, context};
use std::rc::Rc;

pub(crate) struct Wait {
    execution: Rc<context::Execution>,
    previous: SuspensionReason,
}

impl Wait {
    pub(crate) fn enter(reason: SuspensionReason) -> Result<Self> {
        let mounted = context::current().ok_or(Error::OutsideVThread)?;
        let execution = mounted.execution()?;
        execution.data.check()?;
        Ok(Self {
            previous: execution.data.replace_reason(reason),
            execution: Rc::clone(execution),
        })
    }

    pub(crate) fn park(&self, parker: &Parker) -> Result<()> {
        parker.park_after_checkpoint(&self.execution)?;
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
