//! Endpoint counts and disconnection, with value destruction outside metadata locks.

use super::{Receiver, SendError, Sender};
use crate::Result;
use std::sync::Arc;

impl<T> Sender<T> {
    /// Sends or parks for buffer capacity. On error, returns the original value.
    pub fn send(&self, value: T) -> std::result::Result<(), SendError<T>> {
        self.core.send(value, true)
    }
    /// Sends immediately without bypassing queued senders; safe for OS callers.
    pub fn try_send(&self, value: T) -> std::result::Result<(), SendError<T>> {
        self.core.send(value, false)
    }
    /// Rejects further sends; receivers may drain already-buffered values.
    pub fn close(&self) {
        self.core.close();
    }
    /// Whether sending is permanently disconnected or explicitly closed.
    pub fn is_closed(&self) -> bool {
        let state = self.core.state.lock();
        state.closed || state.receivers == 0
    }
    /// Configured message capacity.
    pub fn capacity(&self) -> usize {
        self.core.capacity
    }
    /// Configured outstanding-wait limit per direction, including selected waiters.
    pub fn wait_capacity(&self) -> usize {
        self.core.wait_capacity
    }
    /// Buffered message count, excluding inputs held by waiting senders.
    pub fn len(&self) -> usize {
        self.core.state.lock().values.len()
    }
    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Outstanding sender wait tickets, including selected waiters.
    pub fn waiting(&self) -> usize {
        self.core.state.lock().send_waits.len()
    }
}

impl<T> Receiver<T> {
    /// Configured outstanding-wait limit per direction, including selected waiters.
    pub fn wait_capacity(&self) -> usize {
        self.core.wait_capacity
    }
    /// Receives or parks for a value; returns `Error::Closed` after disconnection
    /// and buffer drain. No value is removed on cancellation or deadline failure.
    pub fn recv(&self) -> Result<T> {
        self.core.recv(true)
    }
    /// Receives immediately without bypassing queued receivers; safe for OS callers.
    pub fn try_recv(&self) -> Result<T> {
        self.core.recv(false)
    }
    /// Rejects further sends while preserving the buffered values for draining.
    pub fn close(&self) {
        self.core.close();
    }
    /// Whether future sends are impossible. Buffered values may still remain.
    pub fn is_closed(&self) -> bool {
        let state = self.core.state.lock();
        state.closed || state.senders == 0
    }
    /// Buffered message count.
    pub fn len(&self) -> usize {
        self.core.state.lock().values.len()
    }
    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Outstanding receiver wait tickets, including selected waiters.
    pub fn waiting(&self) -> usize {
        self.core.state.lock().recv_waits.len()
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        let mut state = self.core.state.lock();
        state.senders = state
            .senders
            .checked_add(1)
            .expect("sender count exhausted");
        Self {
            core: Arc::clone(&self.core),
        }
    }
}
impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        let mut state = self.core.state.lock();
        state.receivers = state
            .receivers
            .checked_add(1)
            .expect("receiver count exhausted");
        Self {
            core: Arc::clone(&self.core),
        }
    }
}
impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut state = self.core.state.lock();
        state.senders -= 1;
        if state.senders == 0 {
            state.wake_all();
        }
    }
}
impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let discarded = {
            let mut state = self.core.state.lock();
            state.receivers -= 1;
            if state.receivers == 0 {
                state.wake_all();
                Some(std::mem::take(&mut state.values))
            } else {
                None
            }
        };
        drop(discarded);
    }
}

#[cfg(test)]
#[path = "endpoints_test.rs"]
mod endpoints_test;
