//! Bounded channel queue tickets preserve FIFO order through cancellation and wake races.

use super::{Core, State};
use crate::{Error, Parker, Result, signal::lock, wait::WaitCell};
use std::collections::VecDeque;

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Send,
    Recv,
}

pub(super) struct Ticket<'a, T> {
    core: &'a Core<T>,
    direction: Direction,
    pub(super) parker: Parker,
    queued: bool,
}

impl<T> State<T> {
    pub(super) fn queue(&mut self, direction: Direction) -> &mut VecDeque<WaitCell> {
        match direction {
            Direction::Send => &mut self.send_waits,
            Direction::Recv => &mut self.recv_waits,
        }
    }

    pub(super) fn wake_fronts(&self) {
        // These only route generation-checked metadata; no user code runs here.
        if let Some(wait) = self.send_waits.front() {
            wait.notify();
        }
        if let Some(wait) = self.recv_waits.front() {
            wait.notify();
        }
    }

    pub(super) fn wake_all(&self) {
        for wait in self.send_waits.iter().chain(&self.recv_waits) {
            wait.notify();
        }
    }
}

impl<'a, T> Ticket<'a, T> {
    pub(super) fn new(core: &'a Core<T>, direction: Direction) -> Self {
        Self {
            core,
            direction,
            parker: Parker {
                wait: WaitCell::new(),
            },
            queued: false,
        }
    }

    pub(super) fn turn(&self, state: &mut State<T>) -> bool {
        state
            .queue(self.direction)
            .front()
            .is_none_or(|wait| self.queued && wait.identity() == self.parker.wait.identity())
    }

    pub(super) fn enqueue(&mut self, state: &mut State<T>) -> Result<()> {
        if self.queued {
            return Ok(());
        }
        let queue = state.queue(self.direction);
        if queue.len() == self.core.wait_capacity {
            return Err(Error::WaitQueueFull {
                limit: self.core.wait_capacity,
            });
        }
        queue.push_back(self.parker.wait.clone());
        self.queued = true;
        Ok(())
    }

    pub(super) fn remove(&mut self, state: &mut State<T>) {
        if !self.queued {
            return;
        }
        let queue = state.queue(self.direction);
        if let Some(index) = queue
            .iter()
            .position(|wait| wait.identity() == self.parker.wait.identity())
        {
            queue.remove(index);
        }
        self.queued = false;
    }
}

impl<T> Drop for Ticket<'_, T> {
    fn drop(&mut self) {
        if !self.queued {
            return;
        }
        let mut state = lock(&self.core.state);
        self.remove(&mut state);
        state.wake_fronts();
    }
}

#[cfg(test)]
#[path = "wait_test.rs"]
mod wait_test;
