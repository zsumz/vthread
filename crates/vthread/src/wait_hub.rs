//! Bounded owner-carrier wake inbox: one reserved slot per active generation.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, Weak},
};

use vthread_stack::ParkToken;

use crate::{
    Error, Result,
    signal::{Signal, lock},
    wait::{WaitRegistration, WaitState, WakeNotice},
};

#[cfg(feature = "runtime-evidence")]
type EvidenceEmitter = crate::diagnostics::evidence::Emitter;
#[cfg(not(feature = "runtime-evidence"))]
type EvidenceEmitter = ();

struct Slot {
    state: Weak<Mutex<WaitState>>,
    #[cfg(feature = "runtime-evidence")]
    task: crate::TaskId,
    selected: bool,
}

#[derive(Default)]
struct HubState {
    slots: BTreeMap<ParkToken, Slot>,
    ready: VecDeque<WakeNotice>,
    stale: u64,
}

pub(crate) struct WaitHub {
    capacity: usize,
    state: Mutex<HubState>,
    signal: Arc<Signal>,
    #[cfg(feature = "runtime-evidence")]
    evidence: Option<crate::diagnostics::evidence::Emitter>,
}

impl WaitHub {
    pub(crate) fn new(capacity: usize, signal: Arc<Signal>) -> Self {
        Self::construct(capacity, signal, None)
    }

    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn with_evidence(
        capacity: usize,
        signal: Arc<Signal>,
        evidence: crate::diagnostics::evidence::Emitter,
    ) -> Self {
        Self::construct(capacity, signal, Some(evidence))
    }

    fn construct(capacity: usize, signal: Arc<Signal>, evidence: Option<EvidenceEmitter>) -> Self {
        #[cfg(not(feature = "runtime-evidence"))]
        let _ = evidence;
        Self {
            capacity,
            state: Mutex::default(),
            signal,
            #[cfg(feature = "runtime-evidence")]
            evidence,
        }
    }

    pub(crate) fn register(
        &self,
        token: ParkToken,
        state: Weak<Mutex<WaitState>>,
        task: crate::TaskId,
    ) -> Result<()> {
        #[cfg(not(feature = "runtime-evidence"))]
        let _ = task;
        let mut hub = lock(&self.state);
        if hub.slots.contains_key(&token) {
            return Err(Error::fault(
                crate::error::FaultComponent::Scheduler,
                "wait token registered twice",
            ));
        }
        if hub.slots.len() >= self.capacity {
            #[cfg(feature = "runtime-evidence")]
            self.record(
                crate::diagnostics::evidence::RuntimeEventKind::AdmissionRejected {
                    resource: crate::error::CapacityResource::Waiters,
                    limit: self.capacity,
                },
            );
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::Waiters,
                limit: self.capacity,
            });
        }
        hub.slots.insert(
            token,
            Slot {
                state,
                #[cfg(feature = "runtime-evidence")]
                task,
                selected: false,
            },
        );
        Ok(())
    }

    pub(crate) fn unregister(&self, token: ParkToken) {
        let mut hub = lock(&self.state);
        let _previous = hub.ready.len();
        hub.slots.remove(&token);
        hub.ready.retain(|notice| notice.token != token);
        let _depth = hub.ready.len();
        #[cfg(feature = "runtime-evidence")]
        if _depth != _previous {
            self.record_depth(_depth);
        }
        drop(hub);
    }

    pub(crate) fn take_registration(&self, token: ParkToken) -> Result<WaitRegistration> {
        let hub = lock(&self.state);
        let slot = hub.slots.get(&token).ok_or(Error::fault(
            crate::error::FaultComponent::Scheduler,
            "park request has no wait registration",
        ))?;
        Ok(WaitRegistration {
            state: slot.state.clone(),
            #[cfg(feature = "runtime-evidence")]
            task: Some(slot.task),
            #[cfg(feature = "runtime-evidence")]
            evidence: self.evidence(),
        })
    }

    pub(crate) fn enqueue(&self, notice: WakeNotice) {
        let mut hub = lock(&self.state);
        let Some(slot) = hub.slots.get_mut(&notice.token) else {
            hub.stale += 1;
            return;
        };
        if slot.selected {
            hub.stale += 1;
            return;
        }
        slot.selected = true;
        // Each queued notice owns a distinct reserved slot, so ready <= slots <= capacity.
        hub.ready.push_back(notice);
        let _depth = hub.ready.len();
        #[cfg(feature = "runtime-evidence")]
        self.record_depth(_depth);
        drop(hub);
        self.signal.notify();
    }

    pub(crate) fn pop_wake(&self) -> Option<WakeNotice> {
        let mut hub = lock(&self.state);
        let notice = hub.ready.pop_front()?;
        hub.slots.remove(&notice.token);
        let _depth = hub.ready.len();
        #[cfg(feature = "runtime-evidence")]
        self.record_depth(_depth);
        drop(hub);
        Some(notice)
    }

    pub(crate) fn pending(&self) -> usize {
        lock(&self.state).ready.len()
    }

    pub(crate) fn pending_tasks(&self) -> Vec<crate::TaskId> {
        lock(&self.state)
            .ready
            .iter()
            .map(|notice| notice.task)
            .collect()
    }

    pub(crate) fn stale(&self) -> u64 {
        lock(&self.state).stale
    }

    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn evidence(&self) -> Option<crate::diagnostics::evidence::Emitter> {
        self.evidence.clone()
    }

    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn record(&self, kind: crate::diagnostics::evidence::RuntimeEventKind) {
        if let Some(evidence) = &self.evidence {
            evidence.record(kind);
        }
    }

    #[cfg(feature = "runtime-evidence")]
    fn record_depth(&self, depth: usize) {
        if let Some(evidence) = &self.evidence {
            evidence.record(crate::diagnostics::evidence::RuntimeEventKind::QueueDepth {
                carrier: evidence.carrier(),
                queue: crate::diagnostics::evidence::QueueKind::Wake,
                depth,
                capacity: self.capacity,
            });
        }
    }
}

#[cfg(test)]
#[path = "wait_hub_test.rs"]
mod wait_hub_test;
