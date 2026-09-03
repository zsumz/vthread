//! Independently published scope admission and retirement progress.

use crate::{TaskId, signal::lock};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

#[repr(align(64))]
struct AdmissionProgress {
    admitted: AtomicU64,
}

#[repr(align(64))]
struct RetirementProgress {
    retired: AtomicU64,
    completed: AtomicU64,
    panicked: AtomicU64,
    aborted: AtomicU64,
    activity: AtomicU64,
}

pub(crate) struct ScopeProgress {
    admission: AdmissionProgress,
    retirement: RetirementProgress,
    failed_tasks: Mutex<Vec<TaskId>>,
}

pub(super) struct ScopeProgressSnapshot {
    pub(super) completed: u64,
    pub(super) panicked: u64,
    pub(super) aborted: u64,
}

impl ScopeProgress {
    pub(super) fn new() -> Self {
        Self {
            admission: AdmissionProgress {
                admitted: AtomicU64::new(0),
            },
            retirement: RetirementProgress {
                retired: AtomicU64::new(0),
                completed: AtomicU64::new(0),
                panicked: AtomicU64::new(0),
                aborted: AtomicU64::new(0),
                activity: AtomicU64::new(0),
            },
            failed_tasks: Mutex::default(),
        }
    }

    pub(super) fn publish_admitted(&self, admitted: u64, record_activity: bool) {
        self.admission.admitted.store(admitted, Ordering::Release);
        if record_activity {
            self.record_activity(1);
        }
    }

    pub(super) fn retire(
        &self,
        count: usize,
        completed: u64,
        panicked: u64,
        aborted: u64,
        failed_tasks: &[TaskId],
        record_activity: bool,
    ) -> bool {
        let count = u64::try_from(count).expect("completion batch fits u64");
        if !failed_tasks.is_empty() {
            lock(&self.failed_tasks).extend_from_slice(failed_tasks);
        }
        add(&self.retirement.completed, completed);
        add(&self.retirement.panicked, panicked);
        add(&self.retirement.aborted, aborted);
        if record_activity {
            self.record_activity(count);
        }
        let retired = self.retirement.retired.fetch_add(count, Ordering::Release) + count;
        retired == self.admission.admitted.load(Ordering::Acquire)
    }

    pub(super) fn active(&self) -> usize {
        let retired = self.retirement.retired.load(Ordering::Acquire);
        let admitted = self.admission.admitted.load(Ordering::Acquire);
        usize::try_from(
            admitted
                .checked_sub(retired)
                .expect("scope retired more tasks than it admitted"),
        )
        .expect("active scope tasks fit usize")
    }

    pub(super) fn activity(&self) -> u64 {
        self.retirement.activity.load(Ordering::Acquire)
    }

    pub(super) fn record_activity(&self, count: u64) {
        self.retirement.activity.fetch_add(count, Ordering::Release);
    }

    pub(super) fn snapshot(&self) -> ScopeProgressSnapshot {
        let _retired = self.retirement.retired.load(Ordering::Acquire);
        ScopeProgressSnapshot {
            completed: self.retirement.completed.load(Ordering::Acquire),
            panicked: self.retirement.panicked.load(Ordering::Acquire),
            aborted: self.retirement.aborted.load(Ordering::Acquire),
        }
    }

    pub(super) fn failed_tasks(&self) -> Vec<TaskId> {
        lock(&self.failed_tasks).clone()
    }

    pub(super) fn retain_failed_tasks(&self, mut keep: impl FnMut(TaskId) -> bool) {
        lock(&self.failed_tasks).retain(|task| keep(*task));
    }
}

fn add(counter: &AtomicU64, value: u64) {
    if value != 0 {
        counter.fetch_add(value, Ordering::Relaxed);
    }
}

#[cfg(test)]
#[path = "control_scope_progress_test.rs"]
mod control_scope_progress_test;
