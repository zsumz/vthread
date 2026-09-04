//! Bounded owner-carrier wake inbox: one queue slot per admitted live task.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use vthread_stack::ParkToken;

use crate::{
    signal::{Signal, lock},
    wait::WakeNotice,
    wake_queue::WakeQueue,
};

#[cfg(feature = "runtime-evidence")]
type EvidenceEmitter = crate::diagnostics::evidence::Emitter;
#[cfg(not(feature = "runtime-evidence"))]
type EvidenceEmitter = ();

pub(crate) struct WaitHub {
    #[cfg(feature = "runtime-evidence")]
    capacity: usize,
    ready: WakeQueue,
    maintenance: Mutex<()>,
    stale: AtomicU64,
    signal: Arc<Signal>,
    tracked_tasks: Option<Mutex<Vec<crate::TaskId>>>,
    #[cfg(test)]
    push_hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    #[cfg(feature = "runtime-evidence")]
    evidence: Option<crate::diagnostics::evidence::Emitter>,
}

impl WaitHub {
    pub(crate) fn new(capacity: usize, signal: Arc<Signal>) -> Self {
        Self::construct(capacity, signal, None, false)
    }

    pub(crate) fn new_tracked(capacity: usize, signal: Arc<Signal>) -> Self {
        Self::construct(capacity, signal, None, true)
    }

    #[cfg(feature = "runtime-evidence")]
    pub(crate) fn with_evidence(
        capacity: usize,
        signal: Arc<Signal>,
        evidence: crate::diagnostics::evidence::Emitter,
        track_tasks: bool,
    ) -> Self {
        Self::construct(capacity, signal, Some(evidence), track_tasks)
    }

    fn construct(
        capacity: usize,
        signal: Arc<Signal>,
        evidence: Option<EvidenceEmitter>,
        track_tasks: bool,
    ) -> Self {
        #[cfg(not(feature = "runtime-evidence"))]
        let _ = evidence;
        Self {
            #[cfg(feature = "runtime-evidence")]
            capacity,
            ready: WakeQueue::new(capacity),
            maintenance: Mutex::default(),
            stale: AtomicU64::new(0),
            signal,
            tracked_tasks: track_tasks.then(|| Mutex::new(Vec::with_capacity(capacity))),
            #[cfg(test)]
            push_hook: Mutex::new(None),
            #[cfg(feature = "runtime-evidence")]
            evidence,
        }
    }

    pub(crate) fn discard_notice(&self, token: ParkToken) {
        let _maintenance = lock(&self.maintenance);
        let initial_depth = self.ready.pending();
        let mut retained = Vec::with_capacity(initial_depth);
        for _ in 0..initial_depth {
            let Some(notice) = self.pop(false) else {
                break;
            };
            if notice.token != token {
                retained.push(notice);
            }
        }
        for notice in retained {
            self.push(notice, false)
                .expect("bounded wake fits while discarding");
        }
        let _depth = self.pending();
        #[cfg(feature = "runtime-evidence")]
        if _depth != initial_depth {
            self.record_depth(_depth);
        }
    }

    pub(crate) fn enqueue(&self, notice: WakeNotice) {
        if self.push(notice, true).is_err() {
            self.stale.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn pop_wake(&self) -> Option<WakeNotice> {
        self.pop(true)
    }

    pub(crate) fn pending(&self) -> usize {
        self.ready.pending()
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.ready.has_pending()
    }

    pub(crate) fn wait(&self, observed: u64, deadline: Option<std::time::Instant>) {
        self.signal
            .wait_while(observed, deadline, || self.ready.arm_wait());
        self.ready.disarm_wait();
    }

    pub(crate) fn pending_tasks(&self) -> Vec<crate::TaskId> {
        self.tracked_tasks
            .as_ref()
            .map_or_else(Vec::new, |tasks| lock(tasks).clone())
    }

    pub(crate) fn stale(&self) -> u64 {
        self.stale.load(Ordering::Relaxed)
    }

    fn push(&self, notice: WakeNotice, record: bool) -> std::result::Result<(), WakeNotice> {
        let task = notice.task;
        let sleeping = self.ready.push(notice, || {
            if let Some(tasks) = &self.tracked_tasks {
                lock(tasks).push(task);
            }
            #[cfg(test)]
            if let Some(hook) = lock(&self.push_hook).take() {
                hook();
            }
        })?;
        #[cfg(feature = "runtime-evidence")]
        if record {
            self.record_depth(self.pending());
        }
        #[cfg(not(feature = "runtime-evidence"))]
        let _ = record;
        if sleeping {
            self.signal.notify_if_waiting();
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn before_pending_publication(&self, hook: impl FnOnce() + Send + 'static) {
        *lock(&self.push_hook) = Some(Box::new(hook));
    }

    fn pop(&self, record: bool) -> Option<WakeNotice> {
        let notice = self.ready.pop()?;
        if let Some(tasks) = &self.tracked_tasks {
            let mut tasks = lock(tasks);
            if let Some(index) = tasks.iter().position(|task| *task == notice.task) {
                tasks.swap_remove(index);
            }
        }
        #[cfg(feature = "runtime-evidence")]
        if record {
            self.record_depth(self.pending());
        }
        #[cfg(not(feature = "runtime-evidence"))]
        let _ = record;
        Some(notice)
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
