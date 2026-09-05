//! Carrier-local runnable queues with bounded cohorts of newest-wake priority.
//!
//! A cohort serves at most two newest wakes, one normal head, then one oldest wake.
//! Missing queues do not consume a dispatch. Serving normal work cannot restart
//! priority before the oldest-wake opportunity. Either persistent queue head is
//! served within four selections; an entry with N older entries in its queue within
//! 4 * (N + 1). These are cooperative dispatch bounds, not wall-clock guarantees.
//! The entry bound assumes normal arrivals join the back; cleanup's push_front
//! explicitly changes a normal entry's rank.

use std::collections::VecDeque;

use crate::task_slab::TaskKey;

const WAKE_BURST: u8 = 2;

pub(crate) struct ReadyQueue {
    normal: VecDeque<TaskKey>,
    wakes: VecDeque<TaskKey>,
    wake_streak: u8,
}

impl ReadyQueue {
    pub(crate) const fn new() -> Self {
        Self {
            normal: VecDeque::new(),
            wakes: VecDeque::new(),
            wake_streak: 0,
        }
    }

    pub(crate) fn push_back(&mut self, task: TaskKey) {
        self.normal.push_back(task);
    }

    pub(crate) fn push_front(&mut self, task: TaskKey) {
        self.normal.push_front(task);
    }

    pub(crate) fn push_wake(&mut self, task: TaskKey) {
        self.wakes.push_front(task);
    }

    pub(crate) fn pop_front(&mut self) -> Option<TaskKey> {
        if self.wakes.is_empty() {
            self.wake_streak = 0;
            return self.normal.pop_front();
        }
        if self.wake_streak < WAKE_BURST {
            self.wake_streak += 1;
            return self.wakes.pop_front();
        }
        if self.wake_streak == WAKE_BURST {
            // Keep the oldest-wake obligation even when normal work is selected.
            self.wake_streak += 1;
            if let Some(task) = self.normal.pop_front() {
                return Some(task);
            }
        }
        self.wake_streak = 0;
        self.wakes.pop_back()
    }

    #[cfg(test)]
    pub(crate) fn front(&self) -> Option<&TaskKey> {
        if self.wakes.is_empty() {
            return self.normal.front();
        }
        if self.wake_streak < WAKE_BURST {
            return self.wakes.front();
        }
        if self.wake_streak == WAKE_BURST {
            return self.normal.front().or_else(|| self.wakes.back());
        }
        self.wakes.back()
    }

    pub(crate) fn len(&self) -> usize {
        self.normal.len() + self.wakes.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.normal.is_empty() && self.wakes.is_empty()
    }
}

#[cfg(test)]
#[path = "ready_queue_test.rs"]
mod ready_queue_test;
