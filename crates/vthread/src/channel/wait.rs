//! Bounded channel queue tickets preserve FIFO order through cancellation and wake races.

use super::{Core, State};
use crate::{Error, Result, wait::WaitCell};
use std::collections::VecDeque;

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Send,
    Recv,
}

pub(super) struct Ticket<'a, T> {
    core: &'a Core<T>,
    direction: Direction,
    wait: Option<u64>,
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
            wait: None,
            queued: false,
        }
    }

    pub(super) fn attach(&mut self, wait: &WaitCell) {
        assert!(
            self.wait.replace(wait.identity()).is_none(),
            "channel wait attached twice"
        );
    }

    pub(super) fn take_turn(&mut self, state: &mut State<T>) -> bool {
        let queue = state.queue(self.direction);
        let Some(front) = queue.front() else {
            assert!(!self.queued, "queued channel ticket disappeared");
            return true;
        };
        if !self.queued || front.identity() != self.wait_identity() {
            return false;
        }
        drop(queue.pop_front().expect("channel queue front"));
        self.queued = false;
        self.wait.take().expect("queued channel ticket");
        true
    }

    pub(super) fn enqueue(&mut self, state: &mut State<T>, wait: &WaitCell) -> Result<()> {
        if self.queued {
            return Ok(());
        }
        assert_eq!(
            wait.identity(),
            self.wait_identity(),
            "channel wait changed"
        );
        if state.queue(self.direction).len() == self.core.wait_capacity {
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::Waiters,
                limit: self.core.wait_capacity,
            });
        }
        state.queue(self.direction).push_back(wait.clone());
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
            .position(|wait| wait.identity() == self.wait_identity())
        {
            drop(queue.remove(index));
        }
        self.queued = false;
        self.wait.take().expect("queued channel ticket");
    }

    fn wait_identity(&self) -> u64 {
        self.wait.expect("attached channel wait")
    }
}

impl<T> Drop for Ticket<'_, T> {
    fn drop(&mut self) {
        if !self.queued {
            return;
        }
        let core = self.core;
        let mut state = core.state.lock();
        self.remove(&mut state);
        state.wake_fronts();
    }
}

#[cfg(test)]
#[path = "wait_test.rs"]
mod wait_test;
