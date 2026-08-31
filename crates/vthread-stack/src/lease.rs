//! Explicit lease transitions keep metadata borrows off executing fiber stacks.

use crate::{Fiber, FiberState, panic_payload};
use corosensei::stack::DefaultStack;
use std::{any::Any, cell::RefCell, rc::Rc};

type Cleanup = Rc<dyn Fn() -> Box<dyn Any>>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Ready,
    Running,
    Complete,
    Reclaiming,
    Reclaimed,
}

struct State {
    phase: Phase,
    fiber: Option<Fiber>,
    cleanup: Option<Cleanup>,
}

/// An internal carrier-local lease; scope exit revokes its executable stack.
#[derive(Clone)]
pub struct FiberLease(Rc<RefCell<State>>);

impl FiberLease {
    pub(crate) fn new(fiber: Fiber) -> Self {
        Self(Rc::new(RefCell::new(State {
            phase: Phase::Ready,
            fiber: Some(fiber),
            cleanup: None,
        })))
    }

    /// Resumes a ready stack. Reclaimed leases return None; reentrant resumes panic.
    pub fn resume(&self) -> Option<FiberState> {
        let fiber = {
            let mut state = self.0.borrow_mut();
            if state.phase == Phase::Reclaimed {
                return None;
            }
            assert!(state.phase == Phase::Ready, "lease is not ready to resume");
            state.phase = Phase::Running;
            state.fiber.take().expect("ready fiber")
        };
        let mut running = Running {
            lease: self,
            fiber: Some(fiber),
        };
        Some(running.fiber.as_mut().expect("running fiber").resume())
    }

    /// Takes a completed stack. Every other phase retains ownership and returns None.
    pub fn take_stack(&self) -> Option<DefaultStack> {
        let (fiber, cleanup) = {
            let mut state = self.0.borrow_mut();
            if state.phase != Phase::Complete {
                return None;
            }
            state.phase = Phase::Reclaimed;
            (state.fiber.take(), state.cleanup.take())
        };
        let stack = fiber.map(Fiber::into_stack);
        drop(cleanup);
        stack
    }

    /// Reclaims a ready/completed stack. Failed mount setup leaves ownership intact.
    /// A running or already reclaiming stack cannot be reclaimed reentrantly.
    pub fn reclaim(&self) {
        let (previous, cleanup) = {
            let mut state = self.0.borrow_mut();
            if state.phase == Phase::Reclaimed {
                return;
            }
            assert!(
                matches!(state.phase, Phase::Ready | Phase::Complete),
                "lease is executing or reclaiming"
            );
            let previous = state.phase;
            state.phase = Phase::Reclaiming;
            (previous, state.cleanup.clone())
        };
        let _restore = Reclaiming {
            lease: self,
            previous,
        };
        // Do not take the fiber until its cleanup context has been mounted successfully.
        let context = cleanup.as_ref().map(|mount| mount());
        let (fiber, retained) = {
            let mut state = self.0.borrow_mut();
            state.phase = Phase::Reclaimed;
            (state.fiber.take(), state.cleanup.take())
        };
        let mut failure = None;
        panic_payload::dispose(fiber, &mut failure);
        panic_payload::dispose(retained, &mut failure);
        panic_payload::dispose(cleanup, &mut failure);
        panic_payload::dispose(context, &mut failure);
        if let Some(failure) = failure {
            std::panic::resume_unwind(Box::new(failure));
        }
    }

    /// Whether the scope retains this fiber, including while it is executing.
    pub fn live(&self) -> bool {
        self.0.borrow().phase != Phase::Reclaimed
    }

    /// Installs the runtime's cleanup mount. Mutating an executing lease panics.
    pub fn cleanup_context(&self, mount: impl Fn() -> Box<dyn Any> + 'static) {
        let replaced = {
            let mut state = self.0.borrow_mut();
            assert!(
                matches!(state.phase, Phase::Ready | Phase::Complete),
                "lease is not idle"
            );
            state.cleanup.replace(Rc::new(mount))
        };
        drop(replaced);
    }
}

struct Running<'a> {
    lease: &'a FiberLease,
    fiber: Option<Fiber>,
}
impl Drop for Running<'_> {
    fn drop(&mut self) {
        let fiber = self.fiber.take().expect("owned running fiber");
        let mut state = self.lease.0.borrow_mut();
        state.phase = if fiber.is_complete() {
            Phase::Complete
        } else {
            Phase::Ready
        };
        state.fiber = Some(fiber);
    }
}

struct Reclaiming<'a> {
    lease: &'a FiberLease,
    previous: Phase,
}
impl Drop for Reclaiming<'_> {
    fn drop(&mut self) {
        let mut state = self.lease.0.borrow_mut();
        if state.phase == Phase::Reclaiming {
            state.phase = self.previous;
        }
    }
}

#[cfg(test)]
#[path = "lease_test.rs"]
mod lease_test;
