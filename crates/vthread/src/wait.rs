//! Modeled wait generations and scheduler registration.

#[path = "wait_select.rs"]
mod wait_select;

use crate::signal::lock;
pub(crate) use crate::wait_hub::WaitHub;
pub(crate) use wait_select::NotifyResult;

use std::{
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use vthread_stack::{ParkRequest, ParkToken};

use crate::{Error, Result, TaskId};

static NEXT_WAIT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WakeCause {
    Ready,
    TimedOut,
    Cancelled,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WakeNotice {
    pub(crate) token: ParkToken,
    pub(crate) task: TaskId,
    pub(crate) cause: WakeCause,
}

pub(crate) enum WaitBegin {
    Immediate(WakeCause),
    Park(ParkRequest),
}

#[derive(Clone)]
pub(crate) struct WaitRegistration {
    pub(crate) state: Weak<Mutex<WaitState>>,
}

#[derive(Clone)]
pub(crate) struct WaitCell {
    state: Arc<Mutex<WaitState>>,
}

pub(crate) struct ParkGuard {
    wait: WaitCell,
    token: ParkToken,
}
impl Drop for ParkGuard {
    fn drop(&mut self) {
        self.wait.rollback(self.token);
    }
}

impl Default for WaitCell {
    fn default() -> Self {
        Self::new()
    }
}

struct ActiveWait {
    token: ParkToken,
    task: TaskId,
    hub: Weak<WaitHub>,
}

pub(crate) struct WaitState {
    id: u64,
    generation: u64,
    permit: bool,
    closed: bool,
    active: Option<ActiveWait>,
    selected: Option<WakeCause>,
}

impl WaitCell {
    pub(crate) fn guard(&self, token: ParkToken) -> ParkGuard {
        ParkGuard {
            wait: self.clone(),
            token,
        }
    }

    pub(crate) fn identity(&self) -> u64 {
        lock(&self.state).id
    }

    pub(crate) fn registration(&self) -> WaitRegistration {
        WaitRegistration {
            state: Arc::downgrade(&self.state),
        }
    }

    pub(crate) fn new() -> Self {
        let id = NEXT_WAIT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .expect("parking identity space exhausted");
        Self {
            state: Arc::new(Mutex::new(WaitState {
                id,
                generation: 0,
                permit: false,
                closed: false,
                active: None,
                selected: None,
            })),
        }
    }

    pub(crate) fn begin(
        &self,
        task: TaskId,
        hub: &Arc<WaitHub>,
        deadline: Option<Instant>,
    ) -> Result<WaitBegin> {
        let mut state = lock(&self.state);
        if state.active.is_some() {
            return Err(Error::ParkerBusy);
        }
        if state.closed {
            return Ok(WaitBegin::Immediate(WakeCause::Closed));
        }
        if state.permit {
            state.permit = false;
            return Ok(WaitBegin::Immediate(WakeCause::Ready));
        }
        if deadline.is_some_and(|deadline| deadline <= Instant::now()) {
            return Ok(WaitBegin::Immediate(WakeCause::TimedOut));
        }

        let generation = state.generation.checked_add(1).ok_or(Error::fault(
            crate::error::FaultComponent::Scheduler,
            "wait generation space exhausted",
        ))?;
        state.generation = generation;
        let token = ParkToken::new(state.id, generation);
        state.active = Some(ActiveWait {
            token,
            task,
            hub: Arc::downgrade(hub),
        });
        state.selected = None;
        if let Err(error) = hub.register(token, Arc::downgrade(&self.state)) {
            state.active = None;
            state.selected = None;
            return Err(error);
        }
        Ok(WaitBegin::Park(ParkRequest::new(token, deadline)))
    }

    pub(crate) fn finish(&self, token: ParkToken) -> Result<WakeCause> {
        let mut state = lock(&self.state);
        let active = state.active.as_ref().ok_or(Error::fault(
            crate::error::FaultComponent::Scheduler,
            "resumed parker has no active wait",
        ))?;
        if active.token != token {
            return Err(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "resumed parker generation changed",
            ));
        }
        let cause = state.selected.take().ok_or(Error::fault(
            crate::error::FaultComponent::Scheduler,
            "resumed parker has no selected wake",
        ))?;
        state.active = None;
        Ok(cause)
    }

    pub(crate) fn rollback(&self, token: ParkToken) {
        let hub = {
            let mut state = lock(&self.state);
            let Some(active) = state.active.as_ref() else {
                return;
            };
            if active.token != token {
                return;
            }
            let hub = active.hub.upgrade();
            state.active = None;
            state.selected = None;
            hub
        };
        if let Some(hub) = hub {
            hub.unregister(token);
        }
    }
}

#[cfg(test)]
#[path = "wait_test.rs"]
mod wait_test;
