//! Carrier-owned stacks, queues, and reclamation. This type is never Send.

#[path = "kernel_cleanup.rs"]
mod kernel_cleanup;
#[path = "kernel_complete.rs"]
mod kernel_complete;
#[path = "kernel_drive.rs"]
mod kernel_drive;
#[path = "kernel_execution.rs"]
mod kernel_execution;
#[path = "kernel_receive.rs"]
mod kernel_receive;
#[path = "kernel_revoked.rs"]
mod kernel_revoked;
#[path = "kernel_task.rs"]
mod kernel_task;
#[path = "kernel_timer.rs"]
mod kernel_timer;
#[path = "parked_tasks.rs"]
mod parked_tasks;

use crate::{
    CarrierId, CarrierSnapshot, CarrierStatus, RuntimeStats, StackSnapshot,
    control::{CompletionBatch, CompletionUpdate, Shared},
    inbox::{Inbox, SpawnPacket},
    kernel_tasks::{KernelTasks, TaskMut, TaskRef},
    ready_queue::ReadyQueue,
    task_slab::TaskKey,
    timer::TimerQueue,
};
use crate::{context::Execution, local_carrier::LocalCarrier};
use parked_tasks::{ParkedTask, ParkedTasks};
use std::{cell::Cell, collections::VecDeque, rc::Rc, sync::Arc};

const PROGRESS_PUBLICATION_BATCH: u8 = 64;
const COMPLETION_BATCH: usize = 64;

pub(crate) struct Kernel {
    pub(super) shared: Arc<Shared>,
    pub(crate) inbox: Arc<Inbox>,
    pub(super) id: CarrierId,
    pub(super) tasks: KernelTasks,
    pub(super) ready: ReadyQueue,
    parked: ParkedTasks,
    pub(super) in_flight: Option<TaskKey>,
    pub(super) pending: Option<SpawnPacket>,
    pub(super) incoming: VecDeque<SpawnPacket>,
    remote_pending: bool,
    pub(super) completions: CompletionBatch,
    completion_progress: Option<(u64, Arc<crate::control::ScopeProgress>)>,
    execution_cache: Vec<Rc<Execution>>,
    pub(crate) local: Rc<LocalCarrier>,
    pub(super) timers: TimerQueue,
    pub(super) stats: RuntimeStats,
    pub(super) has_borrowed: bool,
    observed_borrowed_scope_epoch: u64,
    yield_pressure: u32,
    unpublished_transitions: Cell<u8>,
    #[cfg(test)]
    pub(super) revocation_inspections: usize,
}

impl Kernel {
    pub(crate) fn new(shared: Arc<Shared>, id: CarrierId) -> Self {
        let config = shared.config;
        Self {
            inbox: Arc::clone(&shared.inboxes[id.0]),
            shared,
            id,
            tasks: KernelTasks::new(),
            ready: ReadyQueue::new(),
            parked: ParkedTasks::new(),
            in_flight: None,
            pending: None,
            incoming: VecDeque::new(),
            remote_pending: false,
            completions: CompletionBatch::new(),
            completion_progress: None,
            execution_cache: Vec::new(),
            local: Rc::new(LocalCarrier::new(config)),
            timers: TimerQueue::new(),
            stats: RuntimeStats::default(),
            has_borrowed: false,
            observed_borrowed_scope_epoch: 0,
            yield_pressure: 0,
            unpublished_transitions: Cell::new(0),
            #[cfg(test)]
            revocation_inspections: 0,
        }
    }

    pub(crate) fn execution(&self, task: TaskKey) -> Rc<Execution> {
        Rc::clone(self.task(task).execution())
    }

    pub(super) fn task(&self, key: TaskKey) -> TaskRef<'_> {
        self.tasks.get(key).expect("live task key")
    }

    pub(super) fn task_mut(&mut self, key: TaskKey) -> TaskMut<'_> {
        self.tasks.get_mut(key).expect("live task key")
    }

    pub(super) fn select_ready(&mut self) {
        self.in_flight = self.ready.pop_front();
        crate::context::set_carrier_runnable(
            !self.ready.is_empty()
                || self.remote_pending
                || self.inbox.pending() != 0
                || self.local.pending_starts() != 0,
        );
    }

    pub(super) fn remove_in_flight(&mut self) {
        let key = self.in_flight.take().expect("in-flight task key");
        assert!(self.tasks.remove(key), "live in-flight task");
    }

    pub(super) fn queue_completion(&mut self, completion: CompletionUpdate) {
        assert!(
            self.completions
                .scope()
                .is_none_or(|queued| queued == completion.scope),
            "completion batch crossed scope ownership"
        );
        if self
            .completion_progress
            .as_ref()
            .is_none_or(|(scope, _)| *scope != completion.scope)
        {
            assert!(self.completions.is_empty(), "live completion scope changed");
            self.completion_progress = Some((
                completion.scope,
                self.shared.scope_progress(completion.scope),
            ));
        }
        self.completions.push(completion);
        if self.completions.len() == COMPLETION_BATCH || !self.shared.may_defer_completion() {
            self.flush_completions();
        }
    }

    pub(super) fn flush_completions(&mut self) {
        if self.completions.is_empty() {
            return;
        }
        let (_, progress) = self
            .completion_progress
            .as_ref()
            .expect("completion scope progress");
        self.shared.publish_completions(&self.completions, progress);
        self.completions.clear();
    }

    pub(crate) fn publish(&self, status: CarrierStatus) {
        self.unpublished_transitions.set(0);
        self.shared.publish(self.snapshot(status));
    }

    pub(super) fn publish_transition(&self) {
        let unpublished = self.unpublished_transitions.get() + 1;
        if unpublished == PROGRESS_PUBLICATION_BATCH {
            self.publish(CarrierStatus::Running);
        } else {
            self.unpublished_transitions.set(unpublished);
        }
    }

    pub(crate) fn retire(&self, status: CarrierStatus) {
        #[cfg(test)]
        assert_ne!(
            self.shared
                .carrier_fault
                .load(std::sync::atomic::Ordering::Acquire),
            2,
            "injected carrier retirement failure"
        );
        let mut snapshot = self.snapshot(status);
        snapshot.stacks.cached = 0;
        self.shared.publish(snapshot);
    }

    pub(crate) fn reclaimed(&self) -> bool {
        self.pending.is_none()
            && self.incoming.is_empty()
            && self.in_flight.is_none()
            && self.ready.is_empty()
            && self.parked.is_empty()
            && self.tasks.is_empty()
            && self.completions.is_empty()
            && self.shared.carrier_progress[self.id.0].mounted().is_none()
            && self.local.pending_starts() == 0
            && self.local.pending_wakes() == 0
            && self.inbox.pending() == 0
    }

    fn snapshot(&self, status: CarrierStatus) -> CarrierSnapshot {
        assert_eq!(
            self.tasks.len(),
            self.ready.len() + self.parked.len() + usize::from(self.in_flight.is_some())
        );
        let mut stats = self.stats;
        stats.stale_wakes += self.inbox.hub.stale();
        CarrierSnapshot {
            id: self.id,
            status,
            active: self.tasks.len(),
            runnable: self.ready.len(),
            parked: self.parked.len(),
            timers: self.timers.active_count(),
            pending_starts: self.inbox.pending(),
            pending_wakes: self.local.pending_wakes() + self.inbox.hub.pending(),
            stats,
            stacks: StackSnapshot::from(self.local.stacks.borrow().snapshot()),
        }
    }
}

#[cfg(test)]
#[path = "kernel_test.rs"]
mod kernel_test;
