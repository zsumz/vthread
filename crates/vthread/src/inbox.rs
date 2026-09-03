//! Bounded transferable start packets and coalesced carrier control requests.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
    pending_starts: AtomicUsize,
    stopped: AtomicBool,
    abort_pending: AtomicBool,
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
            pending_starts: AtomicUsize::new(0),
            stopped: AtomicBool::new(false),
            abort_pending: AtomicBool::new(false),
            hub: Arc::new(hub),
            signal,
            #[cfg(feature = "runtime-evidence")]
            evidence,
        }
    }

    pub(crate) fn can_accept(&self) -> bool {
        !self.stopped.load(Ordering::Acquire)
            && self.pending_starts.load(Ordering::Acquire) < self.capacity
    }

    pub(crate) fn push(&self, packet: SpawnPacket) -> std::result::Result<(), SpawnPacket> {
        #[cfg(feature = "runtime-evidence")]
        let record = Arc::clone(&packet.record);
        let mut state = lock(&self.state);
        if state.stopped || state.starts.len() >= self.capacity {
            return Err(packet);
        }
        state.starts.push_back(packet);
        let was_empty = self.pending_starts.fetch_add(1, Ordering::Release) == 0;
        let _depth = state.starts.len();
        #[cfg(feature = "runtime-evidence")]
        {
            self.record_task_accepted(&record);
            self.record_depth(_depth);
        }
        drop(state);
        if was_empty {
            self.signal.notify();
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn pop(&self) -> Option<SpawnPacket> {
        if self.pending_starts.load(Ordering::Acquire) == 0 {
            return None;
        }
        let mut state = lock(&self.state);
        let packet = state.starts.pop_front();
        if packet.is_some() {
            self.pending_starts.fetch_sub(1, Ordering::Release);
        }
        let _depth = state.starts.len();
        #[cfg(feature = "runtime-evidence")]
        if packet.is_some() {
            self.record_depth(_depth);
        }
        drop(state);
        packet
    }

    pub(crate) fn drain_into(&self, packets: &mut VecDeque<SpawnPacket>, limit: usize) -> usize {
        if limit == 0 || self.pending_starts.load(Ordering::Acquire) == 0 {
            return 0;
        }
        let mut state = lock(&self.state);
        let initial_depth = state.starts.len();
        let count = initial_depth.min(limit);
        packets.extend(state.starts.drain(..count));
        if count != 0 {
            self.pending_starts.fetch_sub(count, Ordering::Release);
        }
        #[cfg(feature = "runtime-evidence")]
        {
            let remaining = state.starts.len();
            for depth in (remaining..initial_depth).rev() {
                self.record_depth(depth);
            }
        }
        drop(state);
        count
    }

    pub(crate) fn pop_scope(&self, scope: Option<u64>) -> Option<SpawnPacket> {
        if self.pending_starts.load(Ordering::Acquire) == 0 {
            return None;
        }
        let mut state = lock(&self.state);
        let index = state
            .starts
            .iter()
            .position(|packet| scope.is_none_or(|scope| packet.record.lock().scope == scope))?;
        let packet = state.starts.remove(index);
        self.pending_starts.fetch_sub(1, Ordering::Release);
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
        self.pending_starts.load(Ordering::Acquire)
    }

    pub(crate) fn stop(&self) {
        lock(&self.state).stopped = true;
        self.stopped.store(true, Ordering::Release);
        self.signal.notify();
    }

    pub(crate) fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    pub(crate) fn abort(&self, scope: u64, reason: TaskFailure) {
        let mut state = lock(&self.state);
        state.abort.insert(scope, reason);
        self.abort_pending.store(true, Ordering::Release);
        drop(state);
        self.signal.notify();
    }

    pub(crate) fn clear_abort(&self, scope: u64) {
        let mut state = lock(&self.state);
        state.abort.remove(&scope);
        self.abort_pending
            .store(!state.abort.is_empty(), Ordering::Release);
    }

    pub(crate) fn take_abort(&self) -> Option<(u64, TaskFailure)> {
        if !self.abort_pending.load(Ordering::Acquire) {
            return None;
        }
        let mut state = lock(&self.state);
        let abort = state.abort.pop_first();
        self.abort_pending
            .store(!state.abort.is_empty(), Ordering::Release);
        abort
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
        let record = record.lock();
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
