//! Single-carrier scheduler state and task admission.

#[path = "kernel_drive.rs"]
mod kernel_drive;

use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use vthread_stack::{Fiber, ParkToken, StackPool};

use crate::{
    Error, PanicReport, Result, RuntimeConfig, RuntimeSnapshot, RuntimeStats, StackSnapshot,
    TaskId, TaskStatus,
    join::JoinCell,
    task::{SharedTaskRecord, TaskRecord},
    timer::TimerQueue,
    wait::{WaitHub, WaitRegistration},
};

pub(crate) struct Kernel {
    pub(super) config: RuntimeConfig,
    pub(super) ready: VecDeque<Task>,
    pub(super) parked: BTreeMap<ParkToken, ParkedTask>,
    pub(super) records: BTreeMap<TaskId, SharedTaskRecord>,
    pub(super) stacks: StackPool,
    pub(super) timers: TimerQueue,
    pub(super) hub: Rc<WaitHub>,
    pub(super) next_task: u64,
    pub(super) active: usize,
    pub(super) stats: RuntimeStats,
}

pub(super) struct Task {
    pub(super) fiber: Fiber,
    pub(super) record: SharedTaskRecord,
}

pub(super) struct ParkedTask {
    pub(super) task: Task,
    pub(super) registration: WaitRegistration,
}

pub(crate) struct Spawned<T> {
    pub(crate) id: TaskId,
    pub(crate) name: Rc<str>,
    pub(crate) cell: Rc<RefCell<JoinCell<T>>>,
    pub(crate) record: SharedTaskRecord,
}

impl Kernel {
    pub(crate) fn new(config: RuntimeConfig) -> Self {
        Self {
            config,
            ready: VecDeque::new(),
            parked: BTreeMap::new(),
            records: BTreeMap::new(),
            stacks: StackPool::new(config.stack_size(), config.stack_cache_capacity()),
            timers: TimerQueue::new(),
            hub: Rc::new(WaitHub::new()),
            next_task: 1,
            active: 0,
            stats: RuntimeStats::default(),
        }
    }

    pub(crate) fn spawn<T, F>(&mut self, scope: u64, name: String, entry: F) -> Result<Spawned<T>>
    where
        F: FnOnce() -> T + 'static,
        T: 'static,
    {
        if self.active >= self.config.max_vthreads() {
            self.stats.rejected += 1;
            return Err(Error::AtCapacity {
                limit: self.config.max_vthreads(),
            });
        }
        let name = normalized_name(name)?;
        let stack = self.stacks.acquire().map_err(Error::StackAllocation)?;
        let next_task = self
            .next_task
            .checked_add(1)
            .ok_or(Error::Invariant("task id space exhausted"))?;
        let id = TaskId::new(self.next_task);
        self.next_task = next_task;

        let record = Rc::new(RefCell::new(TaskRecord {
            id,
            scope,
            name: Rc::clone(&name),
            status: TaskStatus::Ready,
            mounts: 0,
            yields: 0,
            parks: 0,
            last_suspension: None,
            last_wake: None,
            outcome_observed: false,
            panic: None,
        }));
        let cell = Rc::new(RefCell::new(JoinCell { outcome: None }));
        let body_record = Rc::clone(&record);
        let body_cell = Rc::clone(&cell);
        let fiber = Fiber::new(stack, move || {
            let outcome = catch_unwind(AssertUnwindSafe(entry));
            match outcome {
                Ok(value) => {
                    body_cell.borrow_mut().outcome = Some(Ok(value));
                    body_record.borrow_mut().status = TaskStatus::Completed;
                }
                Err(payload) => {
                    let panic = PanicReport::capture(payload);
                    body_cell.borrow_mut().outcome = Some(Err(panic.clone()));
                    let mut record = body_record.borrow_mut();
                    record.status = TaskStatus::Panicked;
                    record.panic = Some(panic);
                }
            }
        });

        self.ready.push_back(Task {
            fiber,
            record: Rc::clone(&record),
        });
        let previous = self.records.insert(id, Rc::clone(&record));
        debug_assert!(previous.is_none(), "task identity must be unique");
        self.active += 1;
        self.stats.spawned += 1;
        Ok(Spawned {
            id,
            name,
            cell,
            record,
        })
    }

    pub(crate) fn is_terminal(&self, id: TaskId) -> Result<bool> {
        self.records
            .get(&id)
            .map(|record| record.borrow().status.is_terminal())
            .ok_or(Error::Invariant("join references an unknown task"))
    }

    pub(crate) fn active_in_scope(&self, scope: u64) -> usize {
        self.records
            .values()
            .filter(|record| {
                let record = record.borrow();
                record.scope == scope && !record.status.is_terminal()
            })
            .count()
    }

    pub(crate) fn unobserved_panic(&self, scope: u64) -> Option<(TaskId, String, PanicReport)> {
        self.records.values().find_map(|record| {
            let record = record.borrow();
            let panic = record.panic.clone()?;
            (record.scope == scope && !record.outcome_observed)
                .then(|| (record.id, record.name.to_string(), panic))
        })
    }

    pub(crate) fn abort_scope(&mut self, scope: u64) -> Result<()> {
        let ready_before = self.ready.len();
        self.ready
            .retain(|task| task.record.borrow().scope != scope);
        let mut removed = ready_before - self.ready.len();

        let parked = self
            .parked
            .iter()
            .filter(|(_, parked)| parked.task.record.borrow().scope == scope)
            .map(|(token, _)| *token)
            .collect::<Vec<_>>();
        for token in parked {
            let parked = self.parked.remove(&token).ok_or(Error::Invariant(
                "parked scope task disappeared during abort",
            ))?;
            self.timers.cancel(token);
            parked.registration.abandon(token);
            removed += 1;
        }

        self.active = self.active.checked_sub(removed).ok_or(Error::Invariant(
            "active task count underflow during scope abort",
        ))?;
        self.stats.aborted += removed as u64;
        Ok(())
    }

    pub(crate) fn purge_scope(&mut self, scope: u64) {
        self.records
            .retain(|_, record| record.borrow().scope != scope);
    }

    pub(crate) fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            active: self.active,
            runnable: self.ready.len(),
            parked: self.parked.len(),
            timers: self.timers.active_count(),
            stats: self.stats,
            stacks: StackSnapshot::from(self.stacks.snapshot()),
            tasks: self
                .records
                .values()
                .map(|record| record.borrow().snapshot())
                .collect(),
        }
    }
}

fn normalized_name(name: String) -> Result<Rc<str>> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::invalid_configuration(
            "task name",
            "must not be empty",
        ));
    }
    Ok(Rc::from(name))
}

#[cfg(test)]
#[path = "kernel_test.rs"]
mod kernel_test;
