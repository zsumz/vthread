//! Cold queue inspection preserves lane order and the dispatch cohort.

use super::{ReadyQueue, WAKE_BURST};
use crate::task_slab::TaskKey;

pub(crate) struct Inspection {
    remaining: [usize; 2],
}

impl ReadyQueue {
    pub(crate) fn inspection(&self) -> Inspection {
        Inspection {
            remaining: [self.normal.len(), self.wakes.len()],
        }
    }

    /// Finish this inspection before otherwise mutating the queue. Each original
    /// entry is checked once; retained entries rotate within their original lane.
    /// Removing one match at a time keeps other live tasks visible during cleanup.
    pub(crate) fn remove_matching(
        &mut self,
        inspection: &mut Inspection,
        mut matches: impl FnMut(TaskKey) -> bool,
    ) -> Option<TaskKey> {
        for (queue, remaining) in [&mut self.normal, &mut self.wakes]
            .into_iter()
            .zip(&mut inspection.remaining)
        {
            while *remaining != 0 {
                *remaining -= 1;
                let task = queue.pop_front().expect("uninspected ready entry");
                if matches(task) {
                    return Some(task);
                }
                queue.push_back(task);
            }
        }
        None
    }

    /// Apply the ordinary cohort policy to eligible entries without moving any
    /// skipped entry or letting it consume a dispatch opportunity.
    pub(crate) fn pop_matching(
        &mut self,
        mut eligible: impl FnMut(TaskKey) -> bool,
    ) -> Option<TaskKey> {
        let normal = self.normal.iter().position(|task| eligible(*task));
        let (mut newest, mut oldest) = (None, None);
        for (index, task) in self.wakes.iter().enumerate() {
            if eligible(*task) {
                newest.get_or_insert(index);
                oldest = Some(index);
            }
        }
        let Some(newest) = newest else {
            let normal = normal?;
            self.wake_streak = 0;
            return self.normal.remove(normal);
        };
        if self.wake_streak < WAKE_BURST {
            self.wake_streak += 1;
            return self.wakes.remove(newest);
        }
        if self.wake_streak == WAKE_BURST
            && let Some(normal) = normal
        {
            self.wake_streak += 1;
            return self.normal.remove(normal);
        }
        self.wake_streak = 0;
        self.wakes.remove(oldest.expect("eligible wake"))
    }

    pub(crate) fn remove(&mut self, task: TaskKey) -> bool {
        for queue in [&mut self.normal, &mut self.wakes] {
            if let Some(index) = queue.iter().position(|entry| *entry == task) {
                queue.remove(index);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
#[path = "ready_queue_inspection_test.rs"]
mod ready_queue_inspection_test;
