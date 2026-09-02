//! Lock-free scheduler progress for one carrier-affine task.

use crate::{SuspensionReason, TaskStatus};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[repr(align(64))]
pub(crate) struct TaskProgress {
    running: AtomicBool,
    yielded: AtomicBool,
    mounts: AtomicU64,
    yields: AtomicU64,
}

impl TaskProgress {
    pub(crate) fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            yielded: AtomicBool::new(false),
            mounts: AtomicU64::new(0),
            yields: AtomicU64::new(0),
        }
    }

    pub(crate) fn mount(&self) -> bool {
        // A task never migrates, so its carrier is the only counter writer.
        // Snapshots are readers and only require an atomic, coherent observation.
        let mounts = self.mounts.load(Ordering::Relaxed);
        self.mounts.store(mounts.wrapping_add(1), Ordering::Relaxed);
        self.running.store(true, Ordering::Release);
        mounts == 0
    }

    pub(crate) fn yield_now(&self) {
        let yields = self.yields.load(Ordering::Relaxed);
        self.yields.store(yields.wrapping_add(1), Ordering::Relaxed);
        self.yielded.store(true, Ordering::Relaxed);
        self.unmount();
    }

    pub(crate) fn unmount(&self) {
        self.running.store(false, Ordering::Release);
    }

    pub(crate) fn clear_yield(&self) {
        self.yielded.store(false, Ordering::Release);
    }

    pub(crate) fn status(&self, retained: TaskStatus) -> TaskStatus {
        if matches!(retained, TaskStatus::Ready | TaskStatus::Running) {
            if self.running.load(Ordering::Acquire) {
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

#[cfg(test)]
#[path = "task_progress_test.rs"]
mod task_progress_test;
