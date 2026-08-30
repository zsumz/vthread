//! Carrier-local monotonic timers with immediate removal on cancellation.

use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    time::Instant,
};

use vthread_stack::ParkToken;

#[derive(Default)]
pub(crate) struct TimerQueue {
    deadlines: BTreeSet<(Instant, ParkToken)>,
    active: BTreeMap<ParkToken, Instant>,
}

impl TimerQueue {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn schedule(&mut self, token: ParkToken, deadline: Instant) -> bool {
        let Entry::Vacant(entry) = self.active.entry(token) else {
            return false;
        };
        entry.insert(deadline);
        self.deadlines.insert((deadline, token));
        true
    }

    pub(crate) fn cancel(&mut self, token: ParkToken) -> bool {
        let Some(deadline) = self.active.remove(&token) else {
            return false;
        };
        self.deadlines.remove(&(deadline, token))
    }

    pub(crate) fn active_count(&self) -> usize {
        self.active.len()
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.deadlines.first().map(|(deadline, _)| *deadline)
    }

    pub(crate) fn pop_expired(&mut self, now: Instant) -> Vec<ParkToken> {
        let mut expired = Vec::new();
        while let Some(&(deadline, token)) = self.deadlines.first() {
            if deadline > now {
                break;
            }
            self.deadlines.pop_first();
            self.active.remove(&token);
            expired.push(token);
        }
        expired
    }
}

#[cfg(test)]
#[path = "timer_test.rs"]
mod timer_test;
