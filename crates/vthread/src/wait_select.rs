//! Wake selection for readiness, timeout, cancellation, and close.

use std::sync::{Arc, Mutex};

use vthread_stack::ParkToken;

use crate::{Error, Result, signal::lock};

use super::{SelectionRejection, WaitCell, WaitRegistration, WaitState, WakeCause, WakeNotice};

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
            let hub = Arc::clone(&state.active.as_ref().expect("active wait").hub);
            state.active = None;
            state.selected = None;
            drop(state);
            hub.discard_notice(token);
        }
    }
}

impl WaitCell {
    pub(crate) fn offer_resource(&self, selection: ResourceSelection) -> bool {
        let mut state = lock(&self.state);
        if state.closed || state.selected.is_some() || state.permit || state.resource.is_some() {
            return false;
        }
        state.resource = Some(selection);
        let Some(active) = state.active.as_ref() else {
            state.permit = true;
            return true;
        };
        super::wait_evidence::record_current(active, WakeCause::Ready);
        let notice = WakeNotice {
            token: active.token,
            task: active.task,
            cause: WakeCause::Ready,
        };
        state.selected = Some(WakeCause::Ready);
        let hub = &state.active.as_ref().expect("active wait").hub;
        if !crate::context::enqueue_local_wake(hub, notice) {
            hub.enqueue(notice);
        }
        true
    }

    pub(crate) fn take_resource(&self) -> Option<ResourceSelection> {
        lock(&self.state).resource.take()
    }

    pub(crate) fn notify(&self) -> NotifyResult {
        let mut state = lock(&self.state);
        if state.closed {
            return NotifyResult::Closed;
        }
        let active = state.active.as_ref();
        let Some(active) = active.filter(|_| state.selected.is_none()) else {
            state.permit = true;
            return NotifyResult::Stored;
        };
        super::wait_evidence::record_current(active, WakeCause::Ready);
        let notice = WakeNotice {
            token: active.token,
            task: active.task,
            cause: WakeCause::Ready,
        };
        state.selected = Some(WakeCause::Ready);
        let hub = &state.active.as_ref().expect("active wait").hub;
        if !crate::context::enqueue_local_wake(hub, notice) {
            hub.enqueue(notice);
        }
        NotifyResult::Woke
    }

    pub(crate) fn cancel(&self) -> bool {
        select_current(&self.state, WakeCause::Cancelled)
    }

    pub(crate) fn close(&self) -> bool {
        let mut state = lock(&self.state);
        if state.closed {
            return false;
        }
        state.closed = true;
        state.permit = false;
        if let Some(active) = state.active.as_ref().filter(|_| state.selected.is_none()) {
            super::wait_evidence::record_current(active, WakeCause::Closed);
            let notice = WakeNotice {
                token: active.token,
                task: active.task,
                cause: WakeCause::Closed,
            };
            state.selected = Some(WakeCause::Closed);
            state
                .active
                .as_ref()
                .expect("active wait")
                .hub
                .enqueue(notice);
        }
        true
    }

    pub(crate) fn is_closed(&self) -> bool {
        lock(&self.state).closed
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

fn select_current(state: &Arc<Mutex<WaitState>>, cause: WakeCause) -> bool {
    let mut state = lock(state);
    let active = state.active.as_ref();
    let Some(active) = active.filter(|_| state.selected.is_none()) else {
        return false;
    };
    super::wait_evidence::record_current(active, cause);
    let notice = WakeNotice {
        token: active.token,
        task: active.task,
        cause,
    };
    state.selected = Some(cause);
    state
        .active
        .as_ref()
        .expect("active wait")
        .hub
        .enqueue(notice);
    true
}

fn select_generation(
    state: &Arc<Mutex<WaitState>>,
    registration: &WaitRegistration,
    token: ParkToken,
    cause: WakeCause,
) -> bool {
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
    let notice = WakeNotice {
        token: active.token,
        task: active.task,
        cause,
    };
    state.selected = Some(cause);
    state
        .active
        .as_ref()
        .expect("active wait")
        .hub
        .enqueue(notice);
    true
}

#[cfg(test)]
#[path = "wait_select_test.rs"]
mod wait_select_test;
