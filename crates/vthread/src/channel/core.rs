//! Channel linearization happens under one short metadata lock, before returning.

use super::{
    Core, SendError,
    wait::{Direction, Ticket},
};
use crate::{Error, Result, SuspensionReason, signal::lock, sync::wait::Wait};

impl<T> Core<T> {
    pub(super) fn send(&self, value: T, blocking: bool) -> std::result::Result<(), SendError<T>> {
        let mut value = Some(value);
        let result = self.send_inner(&mut value, blocking);
        result.map_err(|error| SendError {
            error,
            value: value.take().expect("failed send retains value"),
        })
    }

    fn send_inner(&self, value: &mut Option<T>, blocking: bool) -> Result<()> {
        let wait = if blocking {
            Some(Wait::enter(SuspensionReason::ChannelSend)?)
        } else {
            None
        };
        let mut ticket = Ticket::new(self, Direction::Send);
        loop {
            {
                let mut state = lock(&self.state);
                if state.closed || state.receivers == 0 {
                    return Err(Error::Closed);
                }
                if state.values.len() < self.capacity && ticket.turn(&mut state) {
                    ticket.remove(&mut state);
                    state.values.push_back(value.take().expect("send input"));
                    state.wake_fronts();
                    return Ok(());
                }
                if wait.is_none() {
                    return Err(Error::WouldBlock);
                }
                ticket.enqueue(&mut state)?;
            }
            wait.as_ref()
                .expect("blocking send")
                .park(ticket.parker())?;
        }
    }

    pub(super) fn recv(&self, blocking: bool) -> Result<T> {
        let wait = if blocking {
            Some(Wait::enter(SuspensionReason::ChannelRecv)?)
        } else {
            None
        };
        let mut ticket = Ticket::new(self, Direction::Recv);
        loop {
            {
                let mut state = lock(&self.state);
                if !state.values.is_empty() && ticket.turn(&mut state) {
                    ticket.remove(&mut state);
                    let value = state.values.pop_front().expect("nonempty channel");
                    state.wake_fronts();
                    return Ok(value);
                }
                if state.values.is_empty() && (state.closed || state.senders == 0) {
                    return Err(Error::Closed);
                }
                if wait.is_none() {
                    return Err(Error::WouldBlock);
                }
                ticket.enqueue(&mut state)?;
            }
            wait.as_ref()
                .expect("blocking receive")
                .park(ticket.parker())?;
        }
    }

    pub(super) fn close(&self) {
        let mut state = lock(&self.state);
        state.closed = true;
        state.wake_all();
    }
}

#[cfg(test)]
#[path = "core_test.rs"]
mod core_test;
