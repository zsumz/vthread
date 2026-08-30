//! Carrier-owned stacks, queues, and reclamation. This type is never Send.

#[path = "kernel_cleanup.rs"]
mod kernel_cleanup;
#[path = "kernel_drive.rs"]
mod kernel_drive;

use crate::{
    CarrierId, CarrierSnapshot, CarrierStatus, RuntimeStats, StackSnapshot, TaskFailure,
    TaskStatus,
    control::Shared,
    inbox::{Inbox, SpawnPacket},
    task::SharedTaskRecord,
    timer::TimerQueue,
    wait::WaitRegistration,
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
use vthread_stack::{Fiber, ParkToken, StackPool};

pub(crate) struct Kernel {
    pub(super) shared: Arc<Shared>,
    pub(crate) inbox: Arc<Inbox>,
    pub(super) id: CarrierId,
    pub(super) ready: VecDeque<Task>,
    pub(super) parked: BTreeMap<ParkToken, ParkedTask>,
    pub(super) in_flight: Option<Task>,
    pub(super) pending: Option<SpawnPacket>,
    pub(super) stacks: StackPool,
    pub(super) timers: TimerQueue,
    pub(super) stats: RuntimeStats,
}

pub(super) struct Task {
    pub(super) fiber: Option<Fiber>,
    pub(super) record: SharedTaskRecord,
}

pub(super) struct ParkedTask {
    pub(super) task: Task,
    pub(super) registration: WaitRegistration,
}

impl Kernel {
    pub(crate) fn new(shared: Arc<Shared>, id: CarrierId) -> Self {
        let config = shared.config;
        Self {
            inbox: Arc::clone(&shared.inboxes[id.0]),
            shared,
            id,
            ready: VecDeque::new(),
            parked: BTreeMap::new(),
            in_flight: None,
            pending: None,
            stacks: StackPool::new(config.stack_size(), config.stack_cache_capacity()),
            timers: TimerQueue::new(),
            stats: RuntimeStats::default(),
        }
    }

    pub(crate) fn receive(&mut self) {
        for _ in 0..self.shared.config.carrier_queue_capacity() {
            self.pending = self.inbox.pop();
            if self.pending.is_none() {
                break;
            }
            let stack = match self.stacks.acquire() {
                Ok(stack) => stack,
                Err(_) => {
                    self.discard_pending(TaskFailure::StackAllocation);
                    continue;
                }
            };
            let packet = self.pending.as_mut().expect("pending packet");
            let entry = packet.entry.take().expect("unstarted packet entry");
            let fiber = Fiber::new(stack, entry);
            self.shared
                .transition(&packet.record, |record| record.status = TaskStatus::Ready);
            self.in_flight = Some(Task {
                fiber: Some(fiber),
                record: Arc::clone(&packet.record),
            });
            self.pending = None;
            self.ready
                .push_back(self.in_flight.take().expect("new task"));
        }
        self.publish(CarrierStatus::Running);
    }

    pub(crate) fn publish(&self, status: CarrierStatus) {
        self.shared.publish(self.snapshot(status));
    }

    pub(crate) fn retire(&self, status: CarrierStatus) {
        let mut snapshot = self.snapshot(status);
        snapshot.stacks.cached = 0;
        self.shared.publish(snapshot);
    }

    fn snapshot(&self, status: CarrierStatus) -> CarrierSnapshot {
        let mut stats = self.stats;
        stats.stale_wakes += self.inbox.hub.stale();
        CarrierSnapshot {
            id: self.id,
            status,
            active: self.ready.len() + self.parked.len() + usize::from(self.in_flight.is_some()),
            runnable: self.ready.len(),
            parked: self.parked.len(),
            timers: self.timers.active_count(),
            pending_starts: self.inbox.pending(),
            pending_wakes: self.inbox.hub.pending(),
            stats,
            stacks: StackSnapshot::from(self.stacks.snapshot()),
        }
    }
}

#[cfg(test)]
#[path = "kernel_test.rs"]
mod kernel_test;
