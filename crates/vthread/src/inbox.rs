//! Bounded transferable start packets and coalesced carrier control requests.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{
    TaskFailure,
    signal::{Signal, lock},
    task::SharedTaskRecord,
    wait::WaitHub,
};

pub(crate) struct SpawnPacket {
    pub(crate) record: SharedTaskRecord,
    pub(crate) entry: Option<Box<dyn FnOnce() + Send>>,
}

#[derive(Default)]
struct InboxState {
    starts: VecDeque<SpawnPacket>,
    stopped: bool,
    abort: BTreeMap<u64, TaskFailure>,
}

pub(crate) struct Inbox {
    pub(crate) started: AtomicBool,
    pub(crate) scheduler_stopped: AtomicBool,
    pub(crate) reclaimed: AtomicBool,
    capacity: usize,
    state: Mutex<InboxState>,
    pub(crate) signal: Arc<Signal>,
    pub(crate) hub: Arc<WaitHub>,
}

impl Inbox {
    pub(crate) fn new(capacity: usize, wait_capacity: usize) -> Self {
        let signal = Arc::new(Signal::default());
        Self {
            started: AtomicBool::new(false),
            scheduler_stopped: AtomicBool::new(false),
            reclaimed: AtomicBool::new(false),
            capacity,
            state: Mutex::default(),
            hub: Arc::new(WaitHub::new(wait_capacity, Arc::clone(&signal))),
            signal,
        }
    }

    pub(crate) fn can_accept(&self) -> bool {
        let state = lock(&self.state);
        !state.stopped && state.starts.len() < self.capacity
    }

    pub(crate) fn push(&self, packet: SpawnPacket) -> std::result::Result<(), SpawnPacket> {
        let mut state = lock(&self.state);
        if state.stopped || state.starts.len() >= self.capacity {
            return Err(packet);
        }
        state.starts.push_back(packet);
        drop(state);
        self.signal.notify();
        Ok(())
    }

    pub(crate) fn pop(&self) -> Option<SpawnPacket> {
        lock(&self.state).starts.pop_front()
    }

    pub(crate) fn pop_scope(&self, scope: Option<u64>) -> Option<SpawnPacket> {
        let mut state = lock(&self.state);
        let index = state
            .starts
            .iter()
            .position(|packet| scope.is_none_or(|scope| lock(&packet.record).scope == scope))?;
        state.starts.remove(index)
    }

    pub(crate) fn cleanup_complete(&self) -> bool {
        (!self.started.load(Ordering::Acquire)
            || (self.scheduler_stopped.load(Ordering::Acquire)
                && self.reclaimed.load(Ordering::Acquire)))
            && self.pending() == 0
    }

    pub(crate) fn pending(&self) -> usize {
        lock(&self.state).starts.len()
    }

    pub(crate) fn stop(&self) {
        lock(&self.state).stopped = true;
        self.signal.notify();
    }

    pub(crate) fn stopped(&self) -> bool {
        lock(&self.state).stopped
    }

    pub(crate) fn abort(&self, scope: u64, reason: TaskFailure) {
        lock(&self.state).abort.insert(scope, reason);
        self.signal.notify();
    }

    pub(crate) fn clear_abort(&self, scope: u64) {
        lock(&self.state).abort.remove(&scope);
    }

    pub(crate) fn take_abort(&self) -> Option<(u64, TaskFailure)> {
        lock(&self.state).abort.pop_first()
    }
}

#[cfg(test)]
#[path = "inbox_test.rs"]
mod inbox_test;
