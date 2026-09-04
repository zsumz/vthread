//! Carrier-local monotonic timers with immediate removal on cancellation.

use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    time::Instant,
};

use crate::task_slab::TaskKey;
use vthread_stack::ParkToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExpiredTimer {
    pub(crate) task: TaskKey,
    pub(crate) token: ParkToken,
}

#[derive(Clone, Copy)]
struct TimerRegistration {
    deadline: Instant,
    task: TaskKey,
}

#[derive(Default)]
pub(crate) struct TimerQueue {
    deadlines: BTreeSet<(Instant, ParkToken)>,
    active: BTreeMap<ParkToken, TimerRegistration>,
}

impl TimerQueue {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn schedule(&mut self, task: TaskKey, token: ParkToken, deadline: Instant) -> bool {
        let Entry::Vacant(entry) = self.active.entry(token) else {
            return false;
        };
        entry.insert(TimerRegistration { deadline, task });
        self.deadlines.insert((deadline, token));
        true
    }

    pub(crate) fn cancel(&mut self, token: ParkToken) -> bool {
        let Some(registration) = self.active.remove(&token) else {
            return false;
        };
        self.deadlines.remove(&(registration.deadline, token))
    }

    pub(crate) fn active_count(&self) -> usize {
        self.active.len()
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.deadlines.first().map(|(deadline, _)| *deadline)
    }

    pub(crate) fn pop_expired(&mut self, now: Instant) -> Vec<ExpiredTimer> {
        let mut expired = Vec::new();
        while let Some(&(deadline, token)) = self.deadlines.first() {
            if deadline > now {
                break;
            }
            self.deadlines.pop_first();
            let registration = self.active.remove(&token).expect("active timer deadline");
            assert_eq!(registration.deadline, deadline, "active timer deadline");
            expired.push(ExpiredTimer {
                task: registration.task,
                token,
            });
        }
        expired
    }
}

#[cfg(test)]
#[path = "timer_test.rs"]
mod timer_test;
