//! Bounded transferable start packets and coalesced carrier control requests.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
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
    abort: Option<(u64, TaskFailure)>,
}

pub(crate) struct Inbox {
    capacity: usize,
    state: Mutex<InboxState>,
    pub(crate) signal: Arc<Signal>,
    pub(crate) hub: Arc<WaitHub>,
}

impl Inbox {
    pub(crate) fn new(capacity: usize, wait_capacity: usize) -> Self {
        let signal = Arc::new(Signal::default());
        Self {
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

    pub(crate) fn drain(&self, scope: Option<u64>) -> VecDeque<SpawnPacket> {
        let mut state = lock(&self.state);
        let mut removed = VecDeque::new();
        for _ in 0..state.starts.len() {
            let packet = state.starts.pop_front().expect("queued packet");
            if scope.is_none_or(|scope| lock(&packet.record).scope == scope) {
                removed.push_back(packet);
            } else {
                state.starts.push_back(packet);
            }
        }
        removed
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
        lock(&self.state).abort = Some((scope, reason));
        self.signal.notify();
    }

    pub(crate) fn take_abort(&self) -> Option<(u64, TaskFailure)> {
        lock(&self.state).abort.take()
    }
}

#[cfg(test)]
#[path = "inbox_test.rs"]
mod inbox_test;
