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
    parker: Option<Parker>,
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
            parker: None,
            queued: false,
        }
    }

    pub(super) fn attach(&mut self, parker: Parker) {
        assert!(
            self.parker.replace(parker).is_none(),
            "channel wait attached twice"
        );
    }

    pub(super) fn take_turn(&mut self, state: &mut State<T>) -> bool {
        let queue = state.queue(self.direction);
        let Some(front) = queue.front() else {
            assert!(!self.queued, "queued channel ticket disappeared");
            return true;
        };
        if !self.queued || !front.same_cell(&self.parker().wait) {
            return false;
        }
        drop(queue.pop_front().expect("channel queue front"));
        self.queued = false;
        drop(self.parker.take().expect("queued channel ticket"));
        true
    }

    pub(super) fn enqueue(&mut self, state: &mut State<T>) -> Result<()> {
        if self.queued {
            return Ok(());
        }
        if state.queue(self.direction).len() == self.core.wait_capacity {
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::Waiters,
                limit: self.core.wait_capacity,
            });
        }
        state
            .queue(self.direction)
            .push_back(self.parker().wait.clone());
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
            .position(|wait| wait.same_cell(&self.parker().wait))
        {
            drop(queue.remove(index));
        }
        self.queued = false;
        drop(self.parker.take().expect("queued channel ticket"));
    }

    pub(super) fn parker(&self) -> &Parker {
        self.parker.as_ref().expect("queued channel ticket")
    }
}

impl<T> Drop for Ticket<'_, T> {
    fn drop(&mut self) {
        if !self.queued {
            return;
        }
        let core = self.core;
        let mut state = lock(&core.state);
        self.remove(&mut state);
        state.wake_fronts();
    }
}

#[cfg(test)]
#[path = "wait_test.rs"]
mod wait_test;
