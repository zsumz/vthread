//! Shared wait handoff and exact recognition of the task-resident wait slot.

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

    pub(crate) fn take_wait(&self, token: ParkToken) -> Result<Option<WaitRegistration>> {
        if let Some(pending) = self.cold.pending_wait.borrow_mut().take() {
            if pending.token != token {
                return Err(Error::fault(
                    crate::error::FaultComponent::Scheduler,
                    "park request changed task registration generation",
                ));
            }
            return Ok(Some(pending.registration));
        }
        self.synchronization_wait_for(token)?;
        Ok(None)
    }

    pub(crate) fn select_synchronization_timeout(&self, token: ParkToken) -> Result<bool> {
        self.synchronization_wait_for(token)?.select_timeout(token)
    }

    pub(crate) fn abandon_synchronization_wait(&self, token: ParkToken) {
        self.synchronization_wait_for(token)
            .expect("parked synchronization wait identity")
            .abandon(token);
    }

    fn synchronization_wait_for(&self, token: ParkToken) -> Result<&crate::wait::WaitCell> {
        self.cold
            .synchronization_wait
            .get()
            .filter(|wait| wait.matches_generation(token))
            .ok_or(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "park request has no task registration",
            ))
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
