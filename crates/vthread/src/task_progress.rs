//! Bounded-lag scheduler progress for one carrier-affine task.

use crate::{SuspensionReason, TaskId, TaskStatus};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const COUNTER_BATCH: u64 = 64;

/// The mounted task identity stays hot on its single writer carrier.
#[repr(align(64))]
pub(crate) struct CarrierProgress {
    mounted: AtomicU64,
}

impl CarrierProgress {
    pub(crate) fn new() -> Self {
        Self {
            mounted: AtomicU64::new(0),
        }
    }

    fn mount(&self, task: TaskId) {
        self.mounted.store(task.get(), Ordering::Release);
    }

    fn unmount(&self) {
        self.mounted.store(0, Ordering::Release);
    }

    pub(crate) fn mounted(&self) -> Option<TaskId> {
        let task = self.mounted.load(Ordering::Acquire);
        (task != 0).then(|| TaskId::new(task))
    }
}

#[repr(align(64))]
pub(crate) struct TaskProgress {
    yielded: AtomicBool,
    mounts: AtomicU64,
    yields: AtomicU64,
}

impl TaskProgress {
    pub(crate) fn new() -> Self {
        Self {
            yielded: AtomicBool::new(false),
            mounts: AtomicU64::new(0),
            yields: AtomicU64::new(0),
        }
    }

    fn yield_now(&self) {
        self.yielded.store(true, Ordering::Release);
    }

    fn park(&self) {
        self.yielded.store(false, Ordering::Release);
    }

    fn publish(&self, mounts: u64, yields: u64) {
        self.mounts.store(mounts, Ordering::Relaxed);
        self.yields.store(yields, Ordering::Relaxed);
    }

    pub(crate) fn status(&self, retained: TaskStatus, running: bool) -> TaskStatus {
        if matches!(retained, TaskStatus::Ready | TaskStatus::Running) {
            if running {
                TaskStatus::Running
            } else {
                TaskStatus::Ready
            }
        } else {
            retained
        }
    }

    pub(crate) fn mounts(&self) -> u64 {
        self.mounts.load(Ordering::Acquire)
    }

    pub(crate) fn yields(&self) -> u64 {
        self.yields.load(Ordering::Acquire)
    }

    pub(crate) fn last_suspension(
        &self,
        retained: Option<SuspensionReason>,
    ) -> Option<SuspensionReason> {
        if self.yielded.load(Ordering::Acquire) {
            Some(SuspensionReason::YieldNow)
        } else {
            retained
        }
    }
}

pub(crate) struct TaskProgressWriter {
    non_yield_mounts: Cell<u64>,
    yields: Cell<u64>,
    yielded: Cell<bool>,
    started: Cell<bool>,
}

pub(crate) struct TaskProgressUpdate {
    first_yield: bool,
    counters: Option<(u64, u64)>,
}

impl TaskProgressWriter {
    pub(crate) fn new() -> Self {
        Self {
            non_yield_mounts: Cell::new(0),
            yields: Cell::new(0),
            yielded: Cell::new(false),
            started: Cell::new(false),
        }
    }

    pub(crate) fn mount(&self, carrier: &CarrierProgress, task: TaskId) -> bool {
        let first = !self.started.get();
        if first {
            self.started.set(true);
        }
        carrier.mount(task);
        first
    }

    #[inline]
    pub(crate) fn resuming_yield(&self) -> bool {
        self.yielded.get()
    }

    pub(crate) fn yield_now(
        &self,
        carrier: &CarrierProgress,
        publish: impl FnOnce(TaskProgressUpdate),
    ) {
        let yields = self.yields.get().wrapping_add(1);
        self.yields.set(yields);
        let counters = yields
            .is_multiple_of(COUNTER_BATCH)
            .then(|| (self.mounts(), yields));
        let first_yield = !self.yielded.replace(true);
        if first_yield || counters.is_some() {
            publish(TaskProgressUpdate {
                first_yield,
                counters,
            });
        }
        carrier.unmount();
    }

    pub(crate) fn unmount(&self, progress: &TaskProgress, carrier: &CarrierProgress, task: TaskId) {
        if carrier.mounted() == Some(task) {
            self.finish_non_yield_mount();
        }
        progress.publish(self.mounts(), self.yields.get());
        carrier.unmount();
    }

    pub(crate) fn park(&self, progress: &TaskProgress, carrier: &CarrierProgress) {
        self.finish_non_yield_mount();
        progress.publish(self.mounts(), self.yields.get());
        if self.yielded.replace(false) {
            progress.park();
        }
        carrier.unmount();
    }

    fn finish_non_yield_mount(&self) {
        self.non_yield_mounts
            .set(self.non_yield_mounts.get().wrapping_add(1));
    }

    fn mounts(&self) -> u64 {
        // Every finished mount either yields or takes one non-yield transition.
        self.yields.get().wrapping_add(self.non_yield_mounts.get())
    }

    pub(crate) fn reset(&mut self) {
        *self.non_yield_mounts.get_mut() = 0;
        *self.yields.get_mut() = 0;
        *self.yielded.get_mut() = false;
        *self.started.get_mut() = false;
    }
}

impl TaskProgress {
    pub(crate) fn apply(&self, update: TaskProgressUpdate) {
        if let Some((mounts, yields)) = update.counters {
            self.publish(mounts, yields);
        }
        if update.first_yield {
            self.yield_now();
        }
    }

    pub(crate) fn reset(&mut self) {
        *self.yielded.get_mut() = false;
        *self.mounts.get_mut() = 0;
        *self.yields.get_mut() = 0;
    }
}

#[cfg(test)]
#[path = "task_progress_test.rs"]
mod task_progress_test;
