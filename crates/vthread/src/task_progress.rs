//! Bounded-lag scheduler progress for one carrier-affine task.

use crate::{SuspensionReason, TaskStatus};
use std::cell::Cell;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

const RUNNING: u8 = 1 << 0;
const YIELDED: u8 = 1 << 1;
const COUNTER_BATCH: u64 = 64;

#[repr(align(64))]
pub(crate) struct TaskProgress {
    state: AtomicU8,
    mounts: AtomicU64,
    yields: AtomicU64,
}

impl TaskProgress {
    pub(crate) fn new() -> Self {
        Self {
            state: AtomicU8::new(0),
            mounts: AtomicU64::new(0),
            yields: AtomicU64::new(0),
        }
    }

    fn mount(&self, yielded: bool) {
        let yielded = if yielded { YIELDED } else { 0 };
        self.state.store(RUNNING | yielded, Ordering::Release);
    }

    fn yield_now(&self) {
        self.state.store(YIELDED, Ordering::Release);
    }

    fn unmount(&self, yielded: bool) {
        self.state
            .store(if yielded { YIELDED } else { 0 }, Ordering::Release);
    }

    fn park(&self) {
        self.state.store(0, Ordering::Release);
    }

    fn publish(&self, mounts: u64, yields: u64) {
        self.mounts.store(mounts, Ordering::Relaxed);
        self.yields.store(yields, Ordering::Relaxed);
    }

    pub(crate) fn status(&self, retained: TaskStatus) -> TaskStatus {
        if matches!(retained, TaskStatus::Ready | TaskStatus::Running) {
            if self.state.load(Ordering::Acquire) & RUNNING != 0 {
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
        if self.state.load(Ordering::Acquire) & YIELDED != 0 {
            Some(SuspensionReason::YieldNow)
        } else {
            retained
        }
    }
}

pub(crate) struct TaskProgressWriter {
    mounts: Cell<u64>,
    yields: Cell<u64>,
    yielded: Cell<bool>,
}

impl TaskProgressWriter {
    pub(crate) fn new() -> Self {
        Self {
            mounts: Cell::new(0),
            yields: Cell::new(0),
            yielded: Cell::new(false),
        }
    }

    pub(crate) fn mount(&self, progress: &TaskProgress) -> bool {
        let mounts = self.mounts.get();
        self.mounts.set(mounts.wrapping_add(1));
        progress.mount(self.yielded.get());
        mounts == 0
    }

    pub(crate) fn yield_now(&self, progress: &TaskProgress) {
        let yields = self.yields.get().wrapping_add(1);
        self.yields.set(yields);
        self.yielded.set(true);
        if yields.is_multiple_of(COUNTER_BATCH) {
            progress.publish(self.mounts.get(), yields);
        }
        progress.yield_now();
    }

    pub(crate) fn unmount(&self, progress: &TaskProgress) {
        progress.publish(self.mounts.get(), self.yields.get());
        progress.unmount(self.yielded.get());
    }

    pub(crate) fn park(&self, progress: &TaskProgress) {
        progress.publish(self.mounts.get(), self.yields.get());
        self.yielded.set(false);
        progress.park();
    }
}

#[cfg(test)]
#[path = "task_progress_test.rs"]
mod task_progress_test;
