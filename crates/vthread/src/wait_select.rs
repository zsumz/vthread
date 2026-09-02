//! Wake selection for readiness, timeout, cancellation, and close.

use std::sync::{Arc, Mutex, Weak};

use vthread_stack::ParkToken;

use crate::{Error, Result, TaskId, signal::lock};

use super::{
    SelectionRejection, WaitCell, WaitHub, WaitRegistration, WaitState, WakeCause, WakeNotice,
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
        let state = self.state.upgrade().ok_or(Error::fault(
            crate::error::FaultComponent::Scheduler,
            "parked wait state was dropped",
        ))?;
        Ok(select_generation(&state, self, token, WakeCause::TimedOut))
    }

    pub(crate) fn abandon(&self, token: ParkToken) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let mut state = lock(&state);
        if state
            .active
            .as_ref()
            .is_some_and(|active| active.token == token)
        {
            let hub = state
                .active
                .as_ref()
                .and_then(|active| active.hub.upgrade());
            state.active = None;
            state.selected = None;
            drop(state);
            if let Some(hub) = hub {
                hub.unregister(token);
            }
        }
    }
}

impl WaitCell {
    pub(crate) fn notify(&self) -> NotifyResult {
        let dispatch = {
            let mut state = lock(&self.state);
            if state.closed {
                return NotifyResult::Closed;
            }
            let active = state.active.as_ref();
            if let Some(active) = active.filter(|_| state.selected.is_none()) {
                super::wait_evidence::record_current(active, WakeCause::Ready);
                let dispatch = (active.token, active.task, active.hub.clone());
                state.selected = Some(WakeCause::Ready);
                Some(dispatch)
            } else {
                state.permit = true;
                return NotifyResult::Stored;
            }
        };
        dispatch_notice(dispatch, WakeCause::Ready);
        NotifyResult::Woke
    }

    pub(crate) fn cancel(&self) -> bool {
        select_current(&self.state, WakeCause::Cancelled)
    }

    pub(crate) fn close(&self) -> bool {
        let dispatch = {
            let mut state = lock(&self.state);
            if state.closed {
                return false;
            }
            state.closed = true;
            state.permit = false;
            let active = state.active.as_ref();
            if let Some(active) = active.filter(|_| state.selected.is_none()) {
                super::wait_evidence::record_current(active, WakeCause::Closed);
                let dispatch = (active.token, active.task, active.hub.clone());
                state.selected = Some(WakeCause::Closed);
                Some(dispatch)
            } else {
                None
            }
        };
        dispatch_notice(dispatch, WakeCause::Closed);
        true
    }

    pub(crate) fn is_closed(&self) -> bool {
        lock(&self.state).closed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotifyResult {
    Woke,
    Stored,
    Closed,
}

fn select_current(state: &Arc<Mutex<WaitState>>, cause: WakeCause) -> bool {
    let dispatch = {
        let mut state = lock(state);
        let active = state.active.as_ref();
        let Some(active) = active.filter(|_| state.selected.is_none()) else {
            return false;
        };
        super::wait_evidence::record_current(active, cause);
        let dispatch = (active.token, active.task, active.hub.clone());
        state.selected = Some(cause);
        Some(dispatch)
    };
    dispatch_notice(dispatch, cause);
    true
}

fn select_generation(
    state: &Arc<Mutex<WaitState>>,
    registration: &WaitRegistration,
    token: ParkToken,
    cause: WakeCause,
) -> bool {
    let dispatch = {
        let mut state = lock(state);
        let Some(active) = state.active.as_ref() else {
            registration.record_rejected(token, cause, SelectionRejection::NoActive);
            return false;
        };
        if active.token != token {
            registration.record_rejected(token, cause, SelectionRejection::Retired);
            return false;
        }
        if state.selected.is_some() {
            registration.record_rejected(token, cause, SelectionRejection::Selected);
            return false;
        }
        registration.record_selected(token, cause);
        let dispatch = (active.token, active.task, active.hub.clone());
        state.selected = Some(cause);
        Some(dispatch)
    };
    dispatch_notice(dispatch, cause);
    true
}

fn dispatch_notice(dispatch: Option<(ParkToken, TaskId, Weak<WaitHub>)>, cause: WakeCause) {
    let Some((token, task, hub)) = dispatch else {
        return;
    };
    if let Some(hub) = hub.upgrade() {
        hub.enqueue(WakeNotice { token, task, cause });
    }
}

#[cfg(test)]
#[path = "wait_select_test.rs"]
mod wait_select_test;
