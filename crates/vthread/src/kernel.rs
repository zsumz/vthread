//! Single-carrier scheduler kernel.

use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use vthread_stack::{Fiber, FiberState, StackPool, Suspension};

use crate::{
    Error, PanicReport, Result, RuntimeConfig, RuntimeSnapshot, RuntimeStats, StackSnapshot,
    SuspensionReason, TaskId, TaskStatus,
    join::JoinCell,
    task::{SharedTaskRecord, TaskRecord},
};

pub(crate) struct Kernel {
    config: RuntimeConfig,
    ready: VecDeque<Task>,
    records: BTreeMap<TaskId, SharedTaskRecord>,
    stacks: StackPool,
    next_task: u64,
    active: usize,
    stats: RuntimeStats,
}

struct Task {
    fiber: Fiber,
    record: SharedTaskRecord,
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
            records: BTreeMap::new(),
            stacks: StackPool::new(config.stack_size(), config.stack_cache_capacity()),
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
            last_suspension: None,
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
        self.records.insert(id, Rc::clone(&record));
        self.active += 1;
        self.stats.spawned += 1;
        Ok(Spawned {
            id,
            name,
            cell,
            record,
        })
    }

    pub(crate) fn tick(&mut self) -> bool {
        let Some(mut task) = self.ready.pop_front() else {
            return false;
        };
        {
            let mut record = task.record.borrow_mut();
            record.status = TaskStatus::Running;
            record.mounts += 1;
        }
        self.stats.mounts += 1;

        match task.fiber.resume() {
            FiberState::Suspended(Suspension::YieldNow) => {
                let mut record = task.record.borrow_mut();
                record.status = TaskStatus::Ready;
                record.yields += 1;
                record.last_suspension = Some(SuspensionReason::YieldNow);
                drop(record);
                self.stats.yields += 1;
                self.ready.push_back(task);
            }
            FiberState::Complete => {
                let status = task.record.borrow().status;
                if status == TaskStatus::Running {
                    task.record.borrow_mut().status = TaskStatus::Completed;
                }
                match task.record.borrow().status {
                    TaskStatus::Completed => self.stats.completed += 1,
                    TaskStatus::Panicked => self.stats.panicked += 1,
                    _ => {}
                }
                self.active -= 1;
                self.stacks.release(task.fiber.into_stack());
            }
        }
        true
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

    pub(crate) fn purge_scope(&mut self, scope: u64) {
        self.records.retain(|_, record| record.borrow().scope != scope);
    }

    pub(crate) fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            active: self.active,
            runnable: self.ready.len(),
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
