//! Carrier-local monotonic timer queue with explicit cancellation.

use std::{
    cmp::Ordering,
    collections::{BTreeSet, BinaryHeap},
    time::Instant,
};

use vthread_stack::ParkToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimerEntry {
    deadline: Instant,
    token: ParkToken,
}

impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .deadline
            .cmp(&self.deadline)
            .then_with(|| other.token.cmp(&self.token))
    }
}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Default)]
pub(crate) struct TimerQueue {
    heap: BinaryHeap<TimerEntry>,
    active: BTreeSet<ParkToken>,
}

impl TimerQueue {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn schedule(&mut self, token: ParkToken, deadline: Instant) -> bool {
        if !self.active.insert(token) {
            return false;
        }
        self.heap.push(TimerEntry { deadline, token });
        true
    }

    pub(crate) fn cancel(&mut self, token: ParkToken) -> bool {
        self.active.remove(&token)
    }

    pub(crate) fn active_count(&self) -> usize {
        self.active.len()
    }

    pub(crate) fn next_deadline(&mut self) -> Option<Instant> {
        self.prune_cancelled();
        self.heap.peek().map(|entry| entry.deadline)
    }

    pub(crate) fn pop_expired(&mut self, now: Instant) -> Vec<ParkToken> {
        let mut expired = Vec::new();
        loop {
            self.prune_cancelled();
            let Some(entry) = self.heap.peek().copied() else {
                break;
            };
            if entry.deadline > now {
                break;
            }
            let _ = self.heap.pop();
            if self.active.remove(&entry.token) {
                expired.push(entry.token);
            }
        }
        expired
    }

    fn prune_cancelled(&mut self) {
        while self
            .heap
            .peek()
            .is_some_and(|entry| !self.active.contains(&entry.token))
        {
            let _ = self.heap.pop();
        }
    }
}

#[cfg(test)]
#[path = "timer_test.rs"]
mod timer_test;
