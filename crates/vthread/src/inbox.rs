//! Bounded transferable start packets and coalesced carrier control requests.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{
    TaskFailure,
    signal::{Signal, lock},
    task::SharedTaskRecord,
    wait::WaitHub,
};

#[cfg(feature = "runtime-evidence")]
type EvidenceEmitter = crate::diagnostics::evidence::Emitter;
#[cfg(not(feature = "runtime-evidence"))]
type EvidenceEmitter = ();

pub(crate) struct SpawnPacket {
    pub(crate) record: SharedTaskRecord,
    pub(crate) entry: Option<Box<dyn FnOnce() + Send>>,
}

#[derive(Default)]
struct InboxState {
    starts: VecDeque<SpawnPacket>,
    stopped: bool,
    abort: BTreeMap<u64, TaskFailure>,
}

pub(crate) struct Inbox {
    pub(crate) started: AtomicBool,
    pub(crate) scheduler_stopped: AtomicBool,
    pub(crate) reclaimed: AtomicBool,
    capacity: usize,
    state: Mutex<InboxState>,
    pub(crate) signal: Arc<Signal>,
    pub(crate) hub: Arc<WaitHub>,
    #[cfg(feature = "runtime-evidence")]
    evidence: Option<crate::diagnostics::evidence::Emitter>,
}

impl Inbox {
    pub(crate) fn new(capacity: usize, wait_capacity: usize) -> Self {
        Self::construct(capacity, wait_capacity, None)
    }

    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn with_evidence(
        capacity: usize,
        wait_capacity: usize,
        evidence: crate::diagnostics::evidence::Emitter,
    ) -> Self {
        Self::construct(capacity, wait_capacity, Some(evidence))
    }

    fn construct(capacity: usize, wait_capacity: usize, evidence: Option<EvidenceEmitter>) -> Self {
        #[cfg(not(feature = "runtime-evidence"))]
        let _ = evidence;
        let signal = Arc::new(Signal::default());
        #[cfg(feature = "runtime-evidence")]
        let hub = evidence.as_ref().map_or_else(
            || WaitHub::new(wait_capacity, Arc::clone(&signal)),
            |evidence| WaitHub::with_evidence(wait_capacity, Arc::clone(&signal), evidence.clone()),
        );
        #[cfg(not(feature = "runtime-evidence"))]
        let hub = WaitHub::new(wait_capacity, Arc::clone(&signal));
        Self {
            started: AtomicBool::new(false),
            scheduler_stopped: AtomicBool::new(false),
            reclaimed: AtomicBool::new(false),
            capacity,
            state: Mutex::default(),
            hub: Arc::new(hub),
            signal,
            #[cfg(feature = "runtime-evidence")]
            evidence,
        }
    }

    pub(crate) fn can_accept(&self) -> bool {
        let state = lock(&self.state);
        !state.stopped && state.starts.len() < self.capacity
    }

    pub(crate) fn push(&self, packet: SpawnPacket) -> std::result::Result<(), SpawnPacket> {
        #[cfg(feature = "runtime-evidence")]
        let record = Arc::clone(&packet.record);
        let mut state = lock(&self.state);
        if state.stopped || state.starts.len() >= self.capacity {
            return Err(packet);
        }
        state.starts.push_back(packet);
        let _depth = state.starts.len();
        #[cfg(feature = "runtime-evidence")]
        {
            self.record_task_accepted(&record);
            self.record_depth(_depth);
        }
        drop(state);
        self.signal.notify();
        Ok(())
    }

    pub(crate) fn pop(&self) -> Option<SpawnPacket> {
        let mut state = lock(&self.state);
        let packet = state.starts.pop_front();
        let _depth = state.starts.len();
        #[cfg(feature = "runtime-evidence")]
        if packet.is_some() {
            self.record_depth(_depth);
        }
        drop(state);
        packet
    }

    pub(crate) fn pop_scope(&self, scope: Option<u64>) -> Option<SpawnPacket> {
        let mut state = lock(&self.state);
        let index = state
            .starts
            .iter()
            .position(|packet| scope.is_none_or(|scope| lock(&packet.record).scope == scope))?;
        let packet = state.starts.remove(index);
        let _depth = state.starts.len();
        #[cfg(feature = "runtime-evidence")]
        self.record_depth(_depth);
        drop(state);
        packet
    }

    pub(crate) fn cleanup_complete(&self) -> bool {
        (!self.started.load(Ordering::Acquire)
            || (self.scheduler_stopped.load(Ordering::Acquire)
                && self.reclaimed.load(Ordering::Acquire)))
            && self.pending() == 0
    }

    pub(crate) fn pending(&self) -> usize {
        lock(&self.state).starts.len()
    }

    pub(crate) fn stop(&self) {
        lock(&self.state).stopped = true;
        self.signal.notify();
    }

    pub(crate) fn stopped(&self) -> bool {
        lock(&self.state).stopped
    }

    pub(crate) fn abort(&self, scope: u64, reason: TaskFailure) {
        lock(&self.state).abort.insert(scope, reason);
        self.signal.notify();
    }

    pub(crate) fn clear_abort(&self, scope: u64) {
        lock(&self.state).abort.remove(&scope);
    }

    pub(crate) fn take_abort(&self) -> Option<(u64, TaskFailure)> {
        lock(&self.state).abort.pop_first()
    }

    #[cfg(feature = "runtime-evidence")]
    fn record_depth(&self, depth: usize) {
        if let Some(evidence) = &self.evidence {
            evidence.record(crate::diagnostics::evidence::RuntimeEventKind::QueueDepth {
                carrier: evidence.carrier(),
                queue: crate::diagnostics::evidence::QueueKind::Start,
                depth,
                capacity: self.capacity,
            });
        }
    }

    #[cfg(feature = "runtime-evidence")]
    fn record_task_accepted(&self, record: &SharedTaskRecord) {
        let Some(evidence) = &self.evidence else {
            return;
        };
        let record = lock(record);
        evidence.record(
            crate::diagnostics::evidence::RuntimeEventKind::TaskAccepted {
                task: record.id,
                scope: crate::diagnostics::ScopeId::new(record.scope),
                parent: record.parent,
                carrier: record.carrier,
            },
        );
    }
}

#[cfg(test)]
#[path = "inbox_test.rs"]
mod inbox_test;
