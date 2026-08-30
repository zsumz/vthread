//! Wake selection for readiness, timeout, cancellation, and close.

use std::sync::{Arc, Mutex, Weak};

use vthread_stack::ParkToken;

use crate::{Error, Result, TaskId, signal::lock};

use super::{WaitCell, WaitHub, WaitRegistration, WaitState, WakeCause, WakeNotice};

impl WaitRegistration {
    pub(crate) fn select_ready(&self, token: ParkToken) -> bool {
        self.state
            .upgrade()
            .is_some_and(|state| select_generation(&state, token, WakeCause::Ready))
    }

    pub(crate) fn select_closed(&self, token: ParkToken) -> bool {
        self.state
            .upgrade()
            .is_some_and(|state| select_generation(&state, token, WakeCause::Closed))
    }
    pub(crate) fn select_cancelled(&self, token: ParkToken) -> bool {
        self.state
            .upgrade()
            .is_some_and(|state| select_generation(&state, token, WakeCause::Cancelled))
    }

    pub(crate) fn select_timeout(&self, token: ParkToken) -> Result<bool> {
        let state = self
            .state
            .upgrade()
            .ok_or(Error::Invariant("parked wait state was dropped"))?;
        Ok(select_generation(&state, token, WakeCause::TimedOut))
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
            let active = state
                .active
                .as_ref()
                .map(|active| (active.token, active.task, active.hub.clone()));
            if let Some(dispatch) = active.filter(|_| state.selected.is_none()) {
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
            let active = state
                .active
                .as_ref()
                .map(|active| (active.token, active.task, active.hub.clone()));
            if let Some(dispatch) = active.filter(|_| state.selected.is_none()) {
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
        let active = state
            .active
            .as_ref()
            .map(|active| (active.token, active.task, active.hub.clone()));
        let Some(dispatch) = active.filter(|_| state.selected.is_none()) else {
            return false;
        };
        state.selected = Some(cause);
        Some(dispatch)
    };
    dispatch_notice(dispatch, cause);
    true
}

fn select_generation(state: &Arc<Mutex<WaitState>>, token: ParkToken, cause: WakeCause) -> bool {
    let dispatch = {
        let mut state = lock(state);
        let active = state
            .active
            .as_ref()
            .filter(|active| active.token == token)
            .map(|active| (active.token, active.task, active.hub.clone()));
        let Some(dispatch) = active.filter(|_| state.selected.is_none()) else {
            return false;
        };
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
