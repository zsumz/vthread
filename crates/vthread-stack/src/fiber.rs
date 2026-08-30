//! Carrier-local stackful fiber wrapper.

use std::{cell::Cell, error::Error, fmt, ptr, rc::Rc, time::Instant};

use corosensei::{Coroutine, CoroutineResult, Yielder, stack::DefaultStack};

/// Identity for one generation of a parking operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParkToken {
    wait: u64,
    generation: u64,
}

impl ParkToken {
    /// Creates a scheduler-visible token.
    pub fn new(wait: u64, generation: u64) -> Self {
        Self { wait, generation }
    }

    /// Returns the stable parking-object identity.
    pub fn wait(self) -> u64 {
        self.wait
    }

    /// Returns the monotonically increasing wait generation.
    pub fn generation(self) -> u64 {
        self.generation
    }
}

/// Scheduler data supplied when a task parks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParkRequest {
    token: ParkToken,
    deadline: Option<Instant>,
}

impl ParkRequest {
    /// Creates a parking request with an optional monotonic deadline.
    pub fn new(token: ParkToken, deadline: Option<Instant>) -> Self {
        Self { token, deadline }
    }

    /// Returns the wait token.
    pub fn token(&self) -> ParkToken {
        self.token
    }

    /// Returns the monotonic deadline, if one exists.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
}

/// A reason a mounted fiber returned control to its carrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Suspension {
    /// The virtual thread cooperatively yielded its turn.
    YieldNow,
    /// The virtual thread parked on a modeled wait generation.
    Park(ParkRequest),
}

/// The outcome of mounting a fiber once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FiberState {
    /// Execution suspended with the supplied reason.
    Suspended(Suspension),
    /// The fiber returned from its entry function.
    Complete,
}

/// Suspension was requested without a mounted fiber.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuspendError;

impl fmt::Display for SuspendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("no virtual-thread stack is mounted on this carrier")
    }
}

impl Error for SuspendError {}

type RawYielder = Yielder<(), Suspension>;

thread_local! {
    static CURRENT_YIELDER: Cell<*const RawYielder> = const { Cell::new(ptr::null()) };
}

/// One carrier-local stackful execution context.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<vthread_stack::Fiber>();
/// ```
pub struct Fiber {
    coroutine: Option<Coroutine<(), Suspension, ()>>,
    yielder: Rc<Cell<*const RawYielder>>,
}

impl Fiber {
    /// Creates a fiber on an already allocated stack.
    pub fn new<F>(stack: DefaultStack, entry: F) -> Self
    where
        F: FnOnce() + 'static,
    {
        let yielder = Rc::new(Cell::new(ptr::null()));
        let body_yielder = Rc::clone(&yielder);
        let coroutine = Coroutine::with_stack(stack, move |current, ()| {
            let pointer = current as *const RawYielder;
            body_yielder.set(pointer);
            let _mount = MountGuard::install(pointer);
            entry();
        });
        Self {
            coroutine: Some(coroutine),
            yielder,
        }
    }

    /// Mounts the fiber until it suspends or completes.
    pub fn resume(&mut self) -> FiberState {
        let _mount = MountGuard::install(self.yielder.get());
        let coroutine = self
            .coroutine
            .as_mut()
            .expect("a completed fiber cannot be resumed");
        match coroutine.resume(()) {
            CoroutineResult::Yield(reason) => FiberState::Suspended(reason),
            CoroutineResult::Return(()) => FiberState::Complete,
        }
    }

    /// Returns whether the entry function has completed.
    pub fn is_complete(&self) -> bool {
        self.coroutine
            .as_ref()
            .is_none_or(|coroutine| coroutine.done())
    }

    /// Reclaims the stack after completion so it can be reused.
    pub fn into_stack(mut self) -> DefaultStack {
        self.coroutine
            .take()
            .expect("fiber stack already extracted")
            .into_stack()
    }
}

/// Suspends the currently mounted fiber.
pub fn suspend(reason: Suspension) -> Result<(), SuspendError> {
    CURRENT_YIELDER.with(|current| {
        let pointer = current.get();
        if pointer.is_null() {
            return Err(SuspendError);
        }

        // The pointer is carrier-local and restored before leaving this mount.
        // SAFETY: it belongs to the currently mounted, non-Send coroutine.
        unsafe {
            (&*pointer).suspend(reason);
        }
        Ok(())
    })
}

struct MountGuard {
    previous: *const RawYielder,
}

impl MountGuard {
    fn install(pointer: *const RawYielder) -> Self {
        let previous = CURRENT_YIELDER.with(|current| current.replace(pointer));
        Self { previous }
    }
}

impl Drop for MountGuard {
    fn drop(&mut self) {
        CURRENT_YIELDER.with(|current| current.set(self.previous));
    }
}

#[cfg(test)]
#[path = "fiber_test.rs"]
mod fiber_test;
