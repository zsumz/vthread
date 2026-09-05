//! Carrier-local runnable queues with bounded latency priority for selected wakes.

use std::collections::VecDeque;

use crate::task_slab::TaskKey;

const WAKE_BURST: u8 = 32;

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
        if !self.wakes.is_empty() && self.wake_streak < WAKE_BURST {
            self.wake_streak += 1;
            return self.wakes.pop_front();
        }
        if let Some(task) = self.normal.pop_front() {
            self.wake_streak = 0;
            return Some(task);
        }
        let task = self.wakes.pop_back();
        if task.is_some() {
            self.wake_streak = 0;
        }
        task
    }

    #[cfg(test)]
    pub(crate) fn front(&self) -> Option<&TaskKey> {
        if !self.wakes.is_empty() && self.wake_streak < WAKE_BURST {
            return self.wakes.front();
        }
        self.normal.front().or_else(|| self.wakes.back())
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
