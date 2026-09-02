//! Carrier-owned stacks, queues, and reclamation. This type is never Send.

#[path = "kernel_cleanup.rs"]
mod kernel_cleanup;
#[path = "kernel_drive.rs"]
mod kernel_drive;
#[path = "kernel_revoked.rs"]
mod kernel_revoked;

use crate::{
    CarrierId, CarrierSnapshot, CarrierStatus, RuntimeStats, StackSnapshot, TaskFailure,
    TaskStatus,
    control::Shared,
    inbox::{Inbox, SpawnPacket},
    task::SharedTaskRecord,
    timer::TimerQueue,
    wait::WaitRegistration,
};
use crate::{
    context::Execution, local_carrier::LocalCarrier, task_context::TaskContext,
    task_fiber::TaskFiber,
};
use std::{
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    sync::Arc,
};
use vthread_stack::{Fiber, ParkToken};

pub(crate) struct Kernel {
    pub(super) shared: Arc<Shared>,
    pub(crate) inbox: Arc<Inbox>,
    pub(super) id: CarrierId,
    pub(super) ready: VecDeque<Task>,
    pub(super) parked: BTreeMap<ParkToken, ParkedTask>,
    pub(super) in_flight: Option<Task>,
    pub(super) pending: Option<SpawnPacket>,
    pub(crate) local: Rc<LocalCarrier>,
    pub(super) timers: TimerQueue,
    pub(super) stats: RuntimeStats,
}

pub(crate) struct Task {
    pub(crate) fiber: Option<TaskFiber>,
    pub(crate) data: Rc<TaskContext>,
    pub(crate) record: SharedTaskRecord,
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
            local: Rc::new(LocalCarrier::new(config)),
            timers: TimerQueue::new(),
            stats: RuntimeStats::default(),
        }
    }

    pub(crate) fn receive(&mut self) {
        loop {
            let task = self.local.starts.borrow_mut().pop_front();
            let Some(task) = task else {
                break;
            };
            #[cfg(feature = "runtime-evidence")]
            self.shared
                .record(crate::diagnostics::evidence::RuntimeEventKind::QueueDepth {
                    carrier: self.id,
                    queue: crate::diagnostics::evidence::QueueKind::LocalStart,
                    depth: self.local.starts.borrow().len(),
                    capacity: self.shared.config.carrier_queue_capacity(),
                });
            self.shared
                .transition(&task.record, |record| record.status = TaskStatus::Ready);
            self.ready.push_back(task);
        }
        for _ in 0..self.shared.config.carrier_queue_capacity() {
            self.pending = self.inbox.pop();
            if self.pending.is_none() {
                break;
            }
            #[cfg(feature = "runtime-evidence")]
            let acquired = self.local.stacks.borrow_mut().acquire_identified();
            #[cfg(not(feature = "runtime-evidence"))]
            let acquired = self.local.stacks.borrow_mut().acquire();
            #[cfg(feature = "runtime-evidence")]
            let (stack_identity, stack) = match acquired {
                Ok(stack) => stack,
                Err(_) => {
                    self.discard_pending(TaskFailure::StackAllocation);
                    continue;
                }
            };
            #[cfg(not(feature = "runtime-evidence"))]
            let stack = match acquired {
                Ok(stack) => stack,
                Err(_) => {
                    self.discard_pending(TaskFailure::StackAllocation);
                    continue;
                }
            };
            let packet = self.pending.as_mut().expect("pending packet");
            let entry = packet.entry.take().expect("unstarted packet entry");
            let fiber = Fiber::new(stack, entry);
            #[cfg(feature = "runtime-evidence")]
            let task_fiber = TaskFiber::owned(fiber, stack_identity);
            #[cfg(not(feature = "runtime-evidence"))]
            let task_fiber = TaskFiber::owned(fiber);
            #[cfg(feature = "runtime-evidence")]
            let task_id = crate::signal::lock(&packet.record).id;
            self.shared
                .transition(&packet.record, |record| record.status = TaskStatus::Ready);
            self.in_flight = Some(Task {
                fiber: Some(task_fiber),
                data: Rc::new(TaskContext::new(
                    crate::signal::lock(&packet.record).options.clone(),
                    self.shared.config.task_local_capacity(),
                )),
                record: Arc::clone(&packet.record),
            });
            #[cfg(feature = "runtime-evidence")]
            self.shared.record(
                crate::diagnostics::evidence::RuntimeEventKind::StackCheckedOut {
                    task: task_id,
                    stack: crate::diagnostics::evidence::StackId::new(self.id, stack_identity),
                },
            );
            self.pending = None;
            self.ready
                .push_back(self.in_flight.take().expect("new task"));
        }
        self.publish(CarrierStatus::Running);
    }

    pub(crate) fn execution(&self, task: &Task) -> Execution {
        Execution {
            record: Arc::clone(&task.record),
            shared: Arc::clone(&self.shared),
            local: Rc::clone(&self.local),
            data: Rc::clone(&task.data),
        }
    }

    pub(crate) fn publish(&self, status: CarrierStatus) {
        self.shared.publish(self.snapshot(status));
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
            && self.in_flight.is_none()
            && self.ready.is_empty()
            && self.parked.is_empty()
            && self.local.starts.borrow().is_empty()
            && self.inbox.pending() == 0
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
            stacks: StackSnapshot::from(self.local.stacks.borrow().snapshot()),
        }
    }
}

#[cfg(test)]
#[path = "kernel_test.rs"]
mod kernel_test;
