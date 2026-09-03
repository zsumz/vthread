//! Bounded owner-carrier wake inbox: one reserved slot per active generation.

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use vthread_stack::ParkToken;

use crate::{
    Error, Result,
    signal::{Signal, lock},
    wait::WakeNotice,
};

#[cfg(feature = "runtime-evidence")]
type EvidenceEmitter = crate::diagnostics::evidence::Emitter;
#[cfg(not(feature = "runtime-evidence"))]
type EvidenceEmitter = ();

#[derive(Default)]
struct HubState {
    ready: VecDeque<WakeNotice>,
}

pub(crate) struct WaitHub {
    capacity: usize,
    state: Mutex<HubState>,
    reserved: AtomicUsize,
    pending: AtomicUsize,
    stale: AtomicU64,
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
            reserved: AtomicUsize::new(0),
            pending: AtomicUsize::new(0),
            stale: AtomicU64::new(0),
            signal,
            #[cfg(feature = "runtime-evidence")]
            evidence,
        }
    }

    pub(crate) fn reserve(&self) -> Result<()> {
        if self
            .reserved
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |reserved| {
                (reserved < self.capacity).then_some(reserved + 1)
            })
            .is_err()
        {
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
        Ok(())
    }

    pub(crate) fn discard_notice(&self, token: ParkToken) {
        let mut hub = lock(&self.state);
        let _previous = hub.ready.len();
        hub.ready.retain(|notice| notice.token != token);
        let _depth = hub.ready.len();
        self.pending.store(_depth, Ordering::SeqCst);
        #[cfg(feature = "runtime-evidence")]
        if _depth != _previous {
            self.record_depth(_depth);
        }
    }

    pub(crate) fn enqueue(&self, notice: WakeNotice) {
        let mut hub = lock(&self.state);
        if hub.ready.len() >= self.reserved.load(Ordering::Acquire) {
            self.stale.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let was_empty = hub.ready.is_empty();
        // Selection is serialized by WaitState; each notice owns one reservation.
        hub.ready.push_back(notice);
        let _depth = hub.ready.len();
        self.pending.store(_depth, Ordering::Release);
        #[cfg(feature = "runtime-evidence")]
        self.record_depth(_depth);
        drop(hub);
        if was_empty {
            self.signal.notify_if_waiting();
        }
    }

    pub(crate) fn pop_wake(&self) -> Option<WakeNotice> {
        if self.pending.load(Ordering::Acquire) == 0 {
            return None;
        }
        let mut hub = lock(&self.state);
        let notice = hub.ready.pop_front()?;
        let _depth = hub.ready.len();
        self.pending.store(_depth, Ordering::Release);
        #[cfg(feature = "runtime-evidence")]
        self.record_depth(_depth);
        drop(hub);
        Some(notice)
    }

    pub(crate) fn pending(&self) -> usize {
        self.pending.load(Ordering::Acquire)
    }

    pub(crate) fn pending_for_wait(&self) -> usize {
        self.pending.load(Ordering::SeqCst)
    }

    pub(crate) fn pending_tasks(&self) -> Vec<crate::TaskId> {
        lock(&self.state)
            .ready
            .iter()
            .map(|notice| notice.task)
            .collect()
    }

    pub(crate) fn stale(&self) -> u64 {
        self.stale.load(Ordering::Relaxed)
    }

    pub(crate) fn release(&self) {
        let previous = self.reserved.fetch_sub(1, Ordering::AcqRel);
        assert!(previous != 0, "wait reservation released twice");
    }

    #[cfg(test)]
    pub(crate) fn reserved(&self) -> usize {
        self.reserved.load(Ordering::Acquire)
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
