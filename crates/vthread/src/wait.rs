//! Modeled wait generations and scheduler registration.

#[path = "wait_select.rs"]
mod wait_select;

pub(crate) use wait_select::NotifyResult;

use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque, btree_map::Entry},
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use vthread_stack::{ParkRequest, ParkToken};

use crate::{Error, Result, TaskId};

static NEXT_WAIT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WakeCause {
    Ready,
    TimedOut,
    Cancelled,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WakeNotice {
    pub(crate) token: ParkToken,
    pub(crate) task: TaskId,
    pub(crate) cause: WakeCause,
}

pub(crate) enum WaitBegin {
    Immediate(WakeCause),
    Park(ParkRequest),
}

#[derive(Default)]
pub(crate) struct WaitHub {
    registrations: RefCell<BTreeMap<ParkToken, Weak<RefCell<WaitState>>>>,
    wakes: RefCell<VecDeque<WakeNotice>>,
}

impl WaitHub {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn register(&self, token: ParkToken, state: Weak<RefCell<WaitState>>) -> Result<()> {
        let mut registrations = self.registrations.borrow_mut();
        match registrations.entry(token) {
            Entry::Vacant(entry) => {
                entry.insert(state);
                Ok(())
            }
            Entry::Occupied(_) => Err(Error::Invariant("wait token registered twice")),
        }
    }

    fn unregister(&self, token: ParkToken) {
        let _ = self.registrations.borrow_mut().remove(&token);
    }

    pub(crate) fn take_registration(&self, token: ParkToken) -> Result<WaitRegistration> {
        let state = self
            .registrations
            .borrow_mut()
            .remove(&token)
            .ok_or(Error::Invariant("park request has no wait registration"))?;
        Ok(WaitRegistration { state })
    }

    fn enqueue(&self, notice: WakeNotice) {
        self.wakes.borrow_mut().push_back(notice);
    }

    pub(crate) fn pop_wake(&self) -> Option<WakeNotice> {
        self.wakes.borrow_mut().pop_front()
    }
}

pub(crate) struct WaitRegistration {
    state: Weak<RefCell<WaitState>>,
}

#[derive(Clone)]
pub(crate) struct WaitCell {
    state: Rc<RefCell<WaitState>>,
}

impl Default for WaitCell {
    fn default() -> Self {
        Self::new()
    }
}

struct ActiveWait {
    token: ParkToken,
    task: TaskId,
    hub: Weak<WaitHub>,
}

struct WaitState {
    id: u64,
    generation: u64,
    permit: bool,
    closed: bool,
    active: Option<ActiveWait>,
    selected: Option<WakeCause>,
}

impl WaitCell {
    pub(crate) fn new() -> Self {
        let id = NEXT_WAIT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .expect("parking identity space exhausted");
        Self {
            state: Rc::new(RefCell::new(WaitState {
                id,
                generation: 0,
                permit: false,
                closed: false,
                active: None,
                selected: None,
            })),
        }
    }

    pub(crate) fn begin(
        &self,
        task: TaskId,
        hub: &Rc<WaitHub>,
        deadline: Option<Instant>,
    ) -> Result<WaitBegin> {
        let mut state = self.state.borrow_mut();
        if state.active.is_some() {
            return Err(Error::ParkerBusy);
        }
        if state.closed {
            return Ok(WaitBegin::Immediate(WakeCause::Closed));
        }
        if state.permit {
            state.permit = false;
            return Ok(WaitBegin::Immediate(WakeCause::Ready));
        }
        if deadline.is_some_and(|deadline| deadline <= Instant::now()) {
            return Ok(WaitBegin::Immediate(WakeCause::TimedOut));
        }

        let generation = state
            .generation
            .checked_add(1)
            .ok_or(Error::Invariant("wait generation space exhausted"))?;
        state.generation = generation;
        let token = ParkToken::new(state.id, generation);
        state.active = Some(ActiveWait {
            token,
            task,
            hub: Rc::downgrade(hub),
        });
        state.selected = None;
        drop(state);

        if let Err(error) = hub.register(token, Rc::downgrade(&self.state)) {
            self.clear_active(token);
            return Err(error);
        }
        Ok(WaitBegin::Park(ParkRequest::new(token, deadline)))
    }

    pub(crate) fn finish(&self, token: ParkToken) -> Result<WakeCause> {
        let mut state = self.state.borrow_mut();
        let active = state
            .active
            .as_ref()
            .ok_or(Error::Invariant("resumed parker has no active wait"))?;
        if active.token != token {
            return Err(Error::Invariant("resumed parker generation changed"));
        }
        let cause = state
            .selected
            .take()
            .ok_or(Error::Invariant("resumed parker has no selected wake"))?;
        state.active = None;
        Ok(cause)
    }

    pub(crate) fn rollback(&self, token: ParkToken) {
        let hub = {
            let mut state = self.state.borrow_mut();
            let Some(active) = state.active.as_ref() else {
                return;
            };
            if active.token != token {
                return;
            }
            let hub = active.hub.upgrade();
            state.active = None;
            state.selected = None;
            hub
        };
        if let Some(hub) = hub {
            hub.unregister(token);
        }
    }

    fn clear_active(&self, token: ParkToken) {
        let mut state = self.state.borrow_mut();
        if state.active.as_ref().is_some_and(|active| active.token == token) {
            state.active = None;
            state.selected = None;
        }
    }
}

#[cfg(test)]
#[path = "wait_test.rs"]
mod wait_test;
