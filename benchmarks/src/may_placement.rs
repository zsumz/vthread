use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread::{self, ThreadId},
};

pub(crate) struct PairProbe {
    tasks: [Mutex<Option<TaskTrace>>; 2],
}

#[derive(Clone)]
pub(crate) struct TaskTrace {
    first: ThreadId,
    last: ThreadId,
    migrated: bool,
}

impl PairProbe {
    pub(crate) fn new() -> Self {
        Self {
            tasks: [Mutex::new(None), Mutex::new(None)],
        }
    }

    pub(crate) fn record(&self, task: usize, trace: TaskTrace) {
        *self.tasks[task].lock().expect("placement probe poisoned") = Some(trace);
    }
}

impl TaskTrace {
    pub(crate) fn start() -> Self {
        let owner = thread::current().id();
        Self {
            first: owner,
            last: owner,
            migrated: false,
        }
    }

    pub(crate) fn observe(&mut self) {
        let owner = thread::current().id();
        self.migrated |= owner != self.last;
        self.last = owner;
    }
}

pub(crate) fn summarize(probes: &[Arc<PairProbe>]) -> (Vec<(usize, usize)>, Vec<bool>) {
    let mut owners = HashMap::new();
    let mut pair_owners = Vec::with_capacity(probes.len());
    let mut migrations = Vec::with_capacity(probes.len() * 2);
    for probe in probes {
        let tasks = probe.tasks.each_ref().map(|trace| {
            trace
                .lock()
                .expect("placement probe poisoned")
                .clone()
                .expect("placement trace missing")
        });
        let pair = tasks.each_ref().map(|trace| {
            let next = owners.len();
            *owners.entry(trace.last).or_insert(next)
        });
        pair_owners.push((pair[0], pair[1]));
        migrations.extend(tasks.map(|trace| trace.migrated || trace.first != trace.last));
    }
    (pair_owners, migrations)
}

#[cfg(test)]
#[path = "may_placement_test.rs"]
mod may_placement_test;
