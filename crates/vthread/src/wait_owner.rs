//! Nonblocking owner queries; a claimed generation retains its route and resources.

#[cfg(test)]
use super::WaitInner;
use super::{Publication, WaitCell, WaitRegistration};
use vthread_stack::ParkToken;

#[cfg(test)]
impl WaitInner {
    pub(super) fn observe_deferred(&self, token: ParkToken) {
        self.observe_publication(
            super::wait_publication_probe_test::Stage::OwnerDeferred,
            token,
        );
    }

    pub(super) fn observe_completion(&self, token: ParkToken) {
        self.observe_publication(
            super::wait_publication_probe_test::Stage::ClaimPublished,
            token,
        );
    }

    pub(super) fn observe_retirement(&self, token: ParkToken) {
        self.observe_publication(
            super::wait_publication_probe_test::Stage::RetirementDeferred,
            token,
        );
    }
}

impl WaitCell {
    pub(crate) fn publication(&self, token: ParkToken) -> Publication {
        self.state.publication(token)
    }

    pub(crate) fn try_abandon(&self, token: ParkToken) -> bool {
        match self.state.try_retire(token) {
            Ok(Some(hub)) => hub.discard_notice(token),
            Ok(None) => {}
            Err(()) => return false,
        }
        true
    }

    pub(crate) fn try_rollback(&self, token: ParkToken) -> bool {
        match self.state.try_retire(token) {
            Ok(Some(hub)) => {
                crate::context::unregister_local_wake(&hub, token);
                hub.discard_notice(token);
            }
            Ok(None) => {}
            Err(()) => return false,
        }
        true
    }
}

impl WaitRegistration {
    pub(crate) fn publication(&self, token: ParkToken) -> Publication {
        self.state
            .upgrade()
            .map_or(Publication::Stale, |state| state.publication(token))
    }

    pub(crate) fn try_abandon(&self, token: ParkToken) -> bool {
        let Some(state) = self.state.upgrade() else {
            return true;
        };
        match state.try_retire(token) {
            Ok(Some(hub)) => hub.discard_notice(token),
            Ok(None) => {}
            Err(()) => return false,
        }
        true
    }
}

#[cfg(test)]
#[path = "wait_owner_test.rs"]
mod wait_owner_test;
