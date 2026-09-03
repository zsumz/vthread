//! Exact handoff of one pending park registration to the owner kernel.

use super::Execution;
use crate::{Error, Result, wait::WaitRegistration};
use vthread_stack::ParkToken;

pub(super) struct PendingWait {
    pub(super) token: ParkToken,
    pub(super) registration: WaitRegistration,
}

pub(crate) struct WaitPublication<'a> {
    execution: &'a Execution,
    token: ParkToken,
}

impl Execution {
    pub(crate) fn publish_wait(
        &self,
        token: ParkToken,
        registration: WaitRegistration,
    ) -> Result<WaitPublication<'_>> {
        let mut pending = self.cold.pending_wait.borrow_mut();
        if pending.is_some() {
            return Err(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "task published two park registrations",
            ));
        }
        *pending = Some(PendingWait {
            token,
            registration,
        });
        Ok(WaitPublication {
            execution: self,
            token,
        })
    }

    pub(crate) fn take_wait(&self, token: ParkToken) -> Result<WaitRegistration> {
        let pending = self
            .cold
            .pending_wait
            .borrow_mut()
            .take()
            .ok_or(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "park request has no task registration",
            ))?;
        if pending.token != token {
            return Err(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "park request changed task registration generation",
            ));
        }
        Ok(pending.registration)
    }

    fn clear_wait(&self, token: ParkToken) {
        let mut pending = self.cold.pending_wait.borrow_mut();
        if pending
            .as_ref()
            .is_some_and(|pending| pending.token == token)
        {
            *pending = None;
        }
    }
}

impl Drop for WaitPublication<'_> {
    fn drop(&mut self) {
        self.execution.clear_wait(self.token);
    }
}

#[cfg(test)]
#[path = "context_pending_test.rs"]
mod context_pending_test;
