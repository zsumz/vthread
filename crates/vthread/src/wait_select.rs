//! Atomic wake selection for readiness, timeout, cancellation, and close.

use std::sync::Arc;

use vthread_stack::ParkToken;

use crate::{Error, Result};

use super::{
    SelectionRejection, WaitCell, WaitRegistration, WakeCause, WakeNotice, wait_state::Phase,
    wait_target::WaitInner,
};

impl WaitRegistration {
    pub(crate) fn select_ready(&self, token: ParkToken) -> bool {
        let Some(state) = self.state.upgrade() else {
            self.record_rejected(token, WakeCause::Ready, SelectionRejection::NoWait);
            return false;
        };
        select_generation(&state, self, token, WakeCause::Ready)
    }

    pub(crate) fn select_closed(&self, token: ParkToken) -> bool {
        let Some(state) = self.state.upgrade() else {
            self.record_rejected(token, WakeCause::Closed, SelectionRejection::NoWait);
            return false;
        };
        select_generation(&state, self, token, WakeCause::Closed)
    }

    pub(crate) fn select_cancelled(&self, token: ParkToken) -> bool {
        let Some(state) = self.state.upgrade() else {
            self.record_rejected(
                token,
                WakeCause::InheritedCancelled,
                SelectionRejection::NoWait,
            );
            return false;
        };
        select_generation(&state, self, token, WakeCause::InheritedCancelled)
    }

    pub(crate) fn select_timeout(&self, token: ParkToken) -> Result<bool> {
        let state = self.state.upgrade().ok_or_else(|| {
            Error::fault(
                crate::error::FaultComponent::Scheduler,
                "parked wait state was dropped",
            )
        })?;
        Ok(select_generation(&state, self, token, WakeCause::TimedOut))
    }

    pub(crate) fn abandon(&self, token: ParkToken) {
        if let Some(state) = self.state.upgrade()
            && let Some(hub) = state.retire(token)
        {
            hub.discard_notice(token);
        }
    }
}

impl WaitCell {
    pub(crate) fn offer_resource(&self, selection: ResourceSelection) -> bool {
        let mut word = self.state.load();
        loop {
            if word.phase() == Phase::Binding {
                std::hint::spin_loop();
                word = self.state.load();
                continue;
            }
            if word.is_closed()
                || word.phase().not_selectable()
                || word.has_permit()
                || word.resource().is_some()
            {
                return false;
            }
            let active = word.phase() == Phase::Active;
            let next = if active {
                word.with_resource(Some(selection))
                    .claimed(WakeCause::Ready)
            } else {
                word.with_resource(Some(selection)).with_permit(true)
            };
            match self.state.compare_exchange(word, next) {
                Ok(()) => {
                    if active {
                        enqueue_selected(&self.state, next, WakeCause::Ready, None, true);
                    }
                    return true;
                }
                Err(observed) => word = observed,
            }
        }
    }

    pub(crate) fn take_resource(&self) -> Option<ResourceSelection> {
        let mut word = self.state.load();
        loop {
            let resource = word.resource()?;
            match self.state.compare_exchange(word, word.with_resource(None)) {
                Ok(()) => return Some(resource),
                Err(observed) => word = observed,
            }
        }
    }

    pub(crate) fn notify(&self) -> NotifyResult {
        let mut word = self.state.load();
        loop {
            let phase = word.phase();
            if phase == Phase::Binding {
                std::hint::spin_loop();
                word = self.state.load();
                continue;
            }
            if word.is_closed() {
                return NotifyResult::Closed;
            }
            let active = phase == Phase::Active;
            if !active && word.is_claimed() {
                std::hint::spin_loop();
                word = self.state.load();
                continue;
            }
            let next = if active {
                word.claimed(WakeCause::Ready)
            } else {
                word.with_permit(true)
            };
            match self.state.compare_exchange(word, next) {
                Ok(()) => {
                    if active {
                        enqueue_selected(&self.state, next, WakeCause::Ready, None, true);
                        return NotifyResult::Woke;
                    }
                    return NotifyResult::Stored;
                }
                Err(observed) => word = observed,
            }
        }
    }

    pub(crate) fn cancel(&self) -> bool {
        select_current(&self.state, WakeCause::Cancelled)
    }

    pub(crate) fn close(&self) -> bool {
        let mut word = self.state.load();
        loop {
            if word.phase() == Phase::Binding || word.is_claimed() {
                std::hint::spin_loop();
                word = self.state.load();
                continue;
            }
            if word.is_closed() {
                return false;
            }
            let active = word.phase() == Phase::Active;
            let mut next = word.with_closed(true).with_permit(false);
            if active {
                next = next.claimed(WakeCause::Closed);
            }
            match self.state.compare_exchange(word, next) {
                Ok(()) => {
                    if active {
                        enqueue_selected(&self.state, next, WakeCause::Closed, None, false);
                    }
                    return true;
                }
                Err(observed) => word = observed,
            }
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.state.load().is_closed()
    }
}

impl Phase {
    fn not_selectable(self) -> bool {
        !matches!(self, Self::Idle | Self::Active)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResourceSelection {
    Permit,
    Broadcast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotifyResult {
    Woke,
    Stored,
    Closed,
}

fn select_current(state: &Arc<WaitInner>, cause: WakeCause) -> bool {
    let mut word = state.load();
    loop {
        if word.phase() == Phase::Binding {
            std::hint::spin_loop();
            word = state.load();
            continue;
        }
        if word.phase() != Phase::Active {
            return false;
        }
        let claimed = word.claimed(cause);
        match state.compare_exchange(word, claimed) {
            Ok(()) => {
                enqueue_selected(state, claimed, cause, None, false);
                return true;
            }
            Err(observed) => word = observed,
        }
    }
}

fn select_generation(
    state: &Arc<WaitInner>,
    registration: &WaitRegistration,
    token: ParkToken,
    cause: WakeCause,
) -> bool {
    if token.wait() != state.id {
        registration.record_rejected(token, cause, SelectionRejection::Retired);
        return false;
    }
    let mut word = state.load();
    loop {
        if word.phase() == Phase::Binding {
            std::hint::spin_loop();
            word = state.load();
            continue;
        }
        if word.phase() == Phase::Idle {
            registration.record_rejected(token, cause, SelectionRejection::NoActive);
            return false;
        }
        if word.generation() != token.generation() {
            registration.record_rejected(token, cause, SelectionRejection::Retired);
            return false;
        }
        if word.phase() != Phase::Active {
            registration.record_rejected(token, cause, SelectionRejection::Selected);
            return false;
        }
        let claimed = word.claimed(cause);
        match state.compare_exchange(word, claimed) {
            Ok(()) => {
                enqueue_selected(state, claimed, cause, Some(registration), false);
                return true;
            }
            Err(observed) => word = observed,
        }
    }
}

fn enqueue_selected(
    state: &WaitInner,
    claimed: super::wait_state::WaitWord,
    cause: WakeCause,
    registration: Option<&WaitRegistration>,
    local: bool,
) {
    let token = ParkToken::new(state.id, claimed.generation());
    state.with_target(claimed, |task, route, hub| {
        if let Some(registration) = registration {
            registration.record_selected(token, cause);
        } else {
            super::wait_evidence::record_current(task, token, hub, cause);
        }
        let notice = WakeNotice {
            token,
            task,
            route,
            cause,
        };
        if !local || !crate::context::enqueue_local_wake(hub, notice) {
            hub.enqueue(notice);
        }
    });
    state.publish_claim(claimed);
}

#[cfg(test)]
#[path = "wait_select_test.rs"]
mod wait_select_test;
