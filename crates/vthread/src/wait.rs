//! Modeled wait generations and scheduler registration.

#[path = "wait_begin.rs"]
mod wait_begin;
#[path = "wait_evidence.rs"]
mod wait_evidence;
#[path = "wait_select.rs"]
mod wait_select;
#[path = "wait_state.rs"]
mod wait_state;
#[path = "wait_target.rs"]
mod wait_target;

use std::sync::{Arc, Weak, atomic::AtomicU64};

use vthread_stack::{ParkRequest, ParkToken};

pub(crate) use crate::wait_hub::WaitHub;
use crate::{Error, Result, TaskId, task_slab::TaskKey};
use wait_evidence::SelectionRejection;
pub(crate) use wait_select::{NotifyResult, ResourceSelection};
use wait_state::Phase;
pub(crate) use wait_target::WaitInner;

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
    pub(crate) route: TaskKey,
    pub(crate) cause: WakeCause,
}

pub(crate) enum WaitBegin<R = WaitRegistration> {
    Immediate(WakeCause),
    Park {
        request: ParkRequest,
        registration: R,
    },
}

impl<R> WaitBegin<R> {
    pub(crate) fn map_registration<S>(self, map: impl FnOnce(R) -> S) -> WaitBegin<S> {
        match self {
            Self::Immediate(cause) => WaitBegin::Immediate(cause),
            Self::Park {
                request,
                registration,
            } => WaitBegin::Park {
                request,
                registration: map(registration),
            },
        }
    }
}

#[derive(Clone)]
pub(crate) struct WaitRegistration {
    pub(crate) state: Weak<WaitInner>,
    #[cfg(feature = "runtime-evidence")]
    pub(crate) task: Option<TaskId>,
    #[cfg(feature = "runtime-evidence")]
    pub(crate) evidence: Option<crate::diagnostics::evidence::Emitter>,
}

#[derive(Clone)]
pub(crate) struct WaitCell {
    state: Arc<WaitInner>,
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

impl WaitRegistration {
    pub(crate) fn cached(state: &Arc<WaitInner>) -> Self {
        #[cfg(feature = "runtime-evidence")]
        let (task, evidence) = {
            let word = state.load();
            state.with_target(word, |task, _, hub| (Some(task), hub.evidence()))
        };
        Self {
            state: Arc::downgrade(state),
            #[cfg(feature = "runtime-evidence")]
            task,
            #[cfg(feature = "runtime-evidence")]
            evidence,
        }
    }

    pub(crate) fn same_cell(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.state, &other.state)
    }

    pub(crate) fn token(&self, generation: u64) -> Option<ParkToken> {
        let state = self.state.upgrade()?;
        Some(ParkToken::new(state.id, generation))
    }
}

impl WaitCell {
    pub(crate) fn registration(&self) -> WaitRegistration {
        #[cfg(feature = "runtime-evidence")]
        let word = self.state.load();
        #[cfg(feature = "runtime-evidence")]
        let (task, evidence) = if matches!(word.phase(), Phase::Idle | Phase::Binding) {
            (None, None)
        } else {
            self.state
                .with_target(word, |task, _, hub| (Some(task), hub.evidence()))
        };
        WaitRegistration {
            state: Arc::downgrade(&self.state),
            #[cfg(feature = "runtime-evidence")]
            task,
            #[cfg(feature = "runtime-evidence")]
            evidence,
        }
    }

    pub(crate) fn weak_state(&self) -> Weak<WaitInner> {
        Arc::downgrade(&self.state)
    }

    pub(crate) fn matches_state(&self, state: &Weak<WaitInner>) -> bool {
        Arc::as_ptr(&self.state) == state.as_ptr()
    }

    pub(crate) fn guard(&self, token: ParkToken) -> ParkGuard<'_> {
        ParkGuard {
            wait: self,
            token,
            armed: true,
        }
    }

    pub(crate) fn identity(&self) -> u64 {
        self.state.id
    }

    pub(crate) fn matches_generation(&self, token: ParkToken) -> bool {
        let word = self.state.load();
        token.wait() == self.state.id && token.generation() == word.generation()
    }

    #[inline]
    pub(crate) fn same_cell(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }

    pub(crate) fn recycle(&self) -> bool {
        let mut word = self.state.load();
        loop {
            if word.phase() != Phase::Idle || word.is_closed() {
                return false;
            }
            let recycled = word.with_permit(false).with_resource(None);
            match self.state.compare_exchange(word, recycled) {
                Ok(()) => return true,
                Err(observed) => word = observed,
            }
        }
    }

    pub(crate) fn new() -> Self {
        let id = NEXT_WAIT_ID
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |current| current.checked_add(1),
            )
            .expect("parking identity space exhausted");
        Self {
            state: Arc::new(WaitInner::new(id)),
        }
    }

    pub(crate) fn finish(&self, token: ParkToken) -> Result<WakeCause> {
        if token.wait() != self.state.id {
            return Err(resumed_generation_fault());
        }
        loop {
            let word = self.state.load();
            if word.generation() != token.generation() {
                return Err(resumed_generation_fault());
            }
            if word.is_claimed() || word.phase() == Phase::Binding {
                std::hint::spin_loop();
                continue;
            }
            let Some(cause) = word.selected_cause() else {
                return Err(Error::fault(
                    crate::error::FaultComponent::Scheduler,
                    "resumed parker has no selected wake",
                ));
            };
            #[cfg(feature = "runtime-evidence")]
            let resumed = self
                .state
                .with_target(word, |task, _, hub| (task, hub.evidence()));
            if self.state.compare_exchange(word, word.retire()).is_ok() {
                #[cfg(feature = "runtime-evidence")]
                if let (task, Some(evidence)) = resumed {
                    evidence.record(crate::diagnostics::evidence::RuntimeEventKind::Resumed {
                        task,
                        wait: crate::diagnostics::evidence::WaitKey::from_token(token),
                        cause: cause.evidence(),
                    });
                }
                return Ok(cause);
            }
        }
    }

    pub(crate) fn rollback(&self, token: ParkToken) {
        let Some(hub) = self.state.retire(token) else {
            return;
        };
        crate::context::unregister_local_wake(&hub, token);
        hub.discard_notice(token);
    }

    pub(crate) fn abandon(&self, token: ParkToken) {
        if let Some(hub) = self.state.retire(token) {
            hub.discard_notice(token);
        }
    }
}

fn resumed_generation_fault() -> Error {
    Error::fault(
        crate::error::FaultComponent::Scheduler,
        "resumed parker generation changed",
    )
}

#[cfg(test)]
#[path = "wait_test.rs"]
mod wait_test;
