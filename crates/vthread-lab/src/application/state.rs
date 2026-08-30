//! Count owned accepted sockets through queueing, service, rejection and reclamation.

use std::sync::{Arc, Mutex};
use vthread::net::TcpStream;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Counts {
    pub accepted: u64,
    pub rejected: u64,
    pub closed: u64,
    pub requests: u64,
    pub deadlines: u64,
    pub malformed: u64,
    pub disconnected: u64,
    pub pending: usize,
    pub active: usize,
    pub peak_pending: usize,
    pub peak_active: usize,
}

pub(crate) type Shared = Arc<Mutex<Counts>>;

pub(crate) fn change<R>(state: &Shared, body: impl FnOnce(&mut Counts) -> R) -> R {
    body(&mut state.lock().unwrap_or_else(|error| error.into_inner()))
}

pub(crate) struct Connection {
    pub stream: TcpStream,
    pub state: Shared,
    active: bool,
}

impl Connection {
    pub(crate) fn new(stream: TcpStream, state: Shared) -> Self {
        change(&state, |state| {
            state.accepted += 1;
            state.pending += 1;
            state.peak_pending = state.peak_pending.max(state.pending);
        });
        Self {
            stream,
            state,
            active: false,
        }
    }
    pub(crate) fn activate(&mut self) {
        change(&self.state, |state| {
            state.pending -= 1;
            state.active += 1;
            state.peak_active = state.peak_active.max(state.active);
        });
        self.active = true;
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        change(&self.state, |state| {
            if self.active {
                state.active -= 1;
            } else {
                state.pending -= 1;
            }
            state.closed += 1;
        });
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
