//! Channel linearization happens under one short metadata lock, before returning.

use super::{
    Core, SendError,
    wait::{Direction, Ticket},
};
use crate::{Error, Result, SuspensionReason, sync::wait::Wait};

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
        if blocking {
            crate::context::check_current()?;
        }
        let mut wait = None;
        let mut ticket = Ticket::new(self, Direction::Send);
        loop {
            {
                let mut state = self.state.lock();
                if state.closed || state.receivers == 0 {
                    return Err(Error::Closed);
                }
                if state.values.len() < self.capacity && ticket.take_turn(&mut state) {
                    state.values.push_back(value.take().expect("send input"));
                    state.wake_fronts();
                    return Ok(());
                }
                if !blocking {
                    return Err(Error::WouldBlock);
                }
                if wait.is_none() {
                    drop(state);
                    let entered = Wait::enter_after_check(SuspensionReason::ChannelSend)?;
                    ticket.attach(entered.synchronization_wait()?);
                    wait = Some(entered);
                    continue;
                }
                ticket.enqueue(
                    &mut state,
                    wait.as_ref()
                        .expect("blocking send")
                        .attached_synchronization_wait(),
                )?;
            }
            wait.as_ref().expect("blocking send").park_notification()?;
        }
    }

    pub(super) fn recv(&self, blocking: bool) -> Result<T> {
        if blocking {
            crate::context::check_current()?;
        }
        let mut wait = None;
        let mut ticket = Ticket::new(self, Direction::Recv);
        loop {
            {
                let mut state = self.state.lock();
                if !state.values.is_empty() && ticket.take_turn(&mut state) {
                    let value = state.values.pop_front().expect("nonempty channel");
                    state.wake_fronts();
                    return Ok(value);
                }
                if state.values.is_empty() && (state.closed || state.senders == 0) {
                    return Err(Error::Closed);
                }
                if !blocking {
                    return Err(Error::WouldBlock);
                }
                if wait.is_none() {
                    drop(state);
                    let entered = Wait::enter_after_check(SuspensionReason::ChannelRecv)?;
                    ticket.attach(entered.synchronization_wait()?);
                    wait = Some(entered);
                    continue;
                }
                ticket.enqueue(
                    &mut state,
                    wait.as_ref()
                        .expect("blocking receive")
                        .attached_synchronization_wait(),
                )?;
            }
            wait.as_ref()
                .expect("blocking receive")
                .park_notification()?;
        }
    }

    pub(super) fn close(&self) {
        let mut state = self.state.lock();
        state.closed = true;
        state.wake_all();
    }
}

#[cfg(test)]
#[path = "core_test.rs"]
mod core_test;
