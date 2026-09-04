//! Modeled wait generations and scheduler registration.

#[path = "wait_evidence.rs"]
mod wait_evidence;
#[path = "wait_select.rs"]
mod wait_select;

use crate::signal::lock;
pub(crate) use crate::wait_hub::WaitHub;
use wait_evidence::SelectionRejection;
pub(crate) use wait_select::{NotifyResult, ResourceSelection};

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
    InheritedCancelled,
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
    Park {
        request: ParkRequest,
        registration: WaitRegistration,
    },
}

#[derive(Clone)]
pub(crate) struct WaitRegistration {
    pub(crate) state: Weak<Mutex<WaitState>>,
    #[cfg(feature = "runtime-evidence")]
    pub(crate) task: Option<TaskId>,
    #[cfg(feature = "runtime-evidence")]
    pub(crate) evidence: Option<crate::diagnostics::evidence::Emitter>,
}

#[derive(Clone)]
pub(crate) struct WaitCell {
    state: Arc<Mutex<WaitState>>,
}

pub(crate) struct ParkGuard<'a> {
    wait: &'a WaitCell,
    token: ParkToken,
    armed: bool,
}

impl ParkGuard<'_> {
    #[inline]
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}
impl Drop for ParkGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.wait.rollback(self.token);
        }
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
    hub: Arc<WaitHub>,
    #[cfg(feature = "runtime-evidence")]
    evidence: Option<crate::diagnostics::evidence::Emitter>,
}

impl Drop for ActiveWait {
    fn drop(&mut self) {
        self.hub.release();
    }
}

pub(crate) struct WaitState {
    id: u64,
    generation: u64,
    permit: bool,
    closed: bool,
    active: Option<ActiveWait>,
    selected: Option<WakeCause>,
    resource: Option<ResourceSelection>,
}

impl WaitCell {
    pub(crate) fn guard(&self, token: ParkToken) -> ParkGuard<'_> {
        ParkGuard {
            wait: self,
            token,
            armed: true,
        }
    }

    pub(crate) fn identity(&self) -> u64 {
        lock(&self.state).id
    }

    #[inline]
    pub(crate) fn same_cell(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }

    pub(crate) fn recycle(&self) -> bool {
        let mut state = lock(&self.state);
        if state.closed || state.active.is_some() {
            return false;
        }
        state.permit = false;
        state.selected = None;
        state.resource = None;
        true
    }

    #[cfg(test)]
    pub(crate) fn registration(&self) -> WaitRegistration {
        #[cfg(feature = "runtime-evidence")]
        let state = lock(&self.state);
        #[cfg(feature = "runtime-evidence")]
        let active = state.active.as_ref();
        WaitRegistration {
            state: Arc::downgrade(&self.state),
            #[cfg(feature = "runtime-evidence")]
            task: active.map(|active| active.task),
            #[cfg(feature = "runtime-evidence")]
            evidence: active.and_then(|active| active.evidence.clone()),
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
                resource: None,
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
        hub.reserve()?;
        state.active = Some(ActiveWait {
            token,
            task,
            hub: Arc::clone(hub),
            #[cfg(feature = "runtime-evidence")]
            evidence: hub.evidence(),
        });
        state.selected = None;
        #[cfg(feature = "runtime-evidence")]
        hub.record(
            crate::diagnostics::evidence::RuntimeEventKind::WaitPublished {
                task,
                wait: crate::diagnostics::evidence::WaitKey::from_token(token),
                has_deadline: deadline.is_some(),
            },
        );
        let registration = WaitRegistration {
            state: Arc::downgrade(&self.state),
            #[cfg(feature = "runtime-evidence")]
            task: Some(task),
            #[cfg(feature = "runtime-evidence")]
            evidence: hub.evidence(),
        };
        drop(state);
        Ok(WaitBegin::Park {
            request: ParkRequest::new(token, deadline),
            registration,
        })
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
        #[cfg(feature = "runtime-evidence")]
        let task = active.task;
        #[cfg(feature = "runtime-evidence")]
        let evidence = active.evidence.clone();
        let cause = state.selected.take().ok_or(Error::fault(
            crate::error::FaultComponent::Scheduler,
            "resumed parker has no selected wake",
        ))?;
        state.active = None;
        drop(state);
        #[cfg(feature = "runtime-evidence")]
        if let Some(evidence) = evidence {
            evidence.record(crate::diagnostics::evidence::RuntimeEventKind::Resumed {
                task,
                wait: crate::diagnostics::evidence::WaitKey::from_token(token),
                cause: cause.evidence(),
            });
        }
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
            let hub = Arc::clone(&active.hub);
            state.active = None;
            state.selected = None;
            hub
        };
        crate::context::unregister_local_wake(&hub, token);
        hub.discard_notice(token);
    }
}

#[cfg(test)]
#[path = "wait_test.rs"]
mod wait_test;
