//! Borrowed fibers whose executable state is revoked before their lexical scope exits.

use crate::{
    Fiber, FiberLease, MappedStack,
    panic_payload::{self, CapturedPanic},
};
use std::{
    cell::{Cell, RefCell},
    io,
    marker::PhantomData,
    rc::Rc,
};

/// Lexical ownership for a bounded set of borrowed, non-Send stacks.
pub struct FiberScope<'scope, 'env: 'scope> {
    registry: Rc<Registry>,
    scope: PhantomData<&'scope mut &'scope ()>,
    env: PhantomData<&'env mut &'env ()>,
}

struct Registry {
    fibers: RefCell<Vec<FiberLease>>,
    capacity: usize,
    closed: Cell<bool>,
}

struct ScopeGuard(Rc<Registry>);

impl<'scope, 'env> FiberScope<'scope, 'env> {
    /// Creates a local stack. The scope retains ownership even if its lease is forgotten.
    pub fn spawn(
        &'scope self,
        stack: MappedStack,
        entry: impl FnOnce() + 'scope,
    ) -> io::Result<FiberLease> {
        let mut fibers = self.registry.fibers.borrow_mut();
        fibers.retain(FiberLease::live);
        if self.registry.closed.get() || fibers.len() >= self.registry.capacity {
            return Err(io::Error::other("borrowed fiber scope closed or full"));
        }
        // SAFETY: entry lives for scope; the non-forgettable outer scope owns a lease
        // and revokes every stack before returning, including on unwind or leaked leases.
        let fiber = unsafe { Fiber::borrowed(stack, entry) };
        let lease = FiberLease::new(fiber);
        fibers.push(lease.clone());
        Ok(lease)
    }
}

impl ScopeGuard {
    fn drain(&self) -> Option<CapturedPanic> {
        self.0.closed.set(true);
        let fibers = self.0.fibers.take();
        let mut failure = None;
        for fiber in fibers {
            for _ in 0..2 {
                if !fiber.live() {
                    break;
                }
                if let Err(payload) =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fiber.reclaim()))
                {
                    panic_payload::retain(&mut failure, payload);
                }
            }
            // A borrowed executable stack must never survive the lexical environment.
            // A mount that fails twice has no safe recovery path; do not leak the fiber.
            if fiber.live() {
                std::process::abort();
            }
        }
        failure
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        self.drain();
    }
}

/// Runs a lexical borrowed-stack owner; leaked leases are inert after this returns.
pub fn fiber_scope<'env, R>(
    capacity: usize,
    body: impl for<'scope> FnOnce(&'scope FiberScope<'scope, 'env>) -> R,
) -> R {
    let registry = Rc::new(Registry {
        fibers: RefCell::new(Vec::new()),
        capacity,
        closed: Cell::new(false),
    });
    let scope = FiberScope {
        registry,
        scope: PhantomData,
        env: PhantomData,
    };
    // Declare the guard last: children may borrow the scope itself, so their
    // reclamation must precede destruction of the scope value on every exit path.
    let guard = ScopeGuard(Rc::clone(&scope.registry));
    // Reclaim outside an active body unwind so a cleanup panic cannot double-unwind.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&scope)));
    let cleanup = guard.drain();
    match result {
        Err(payload) => {
            // corosensei uses a private panic payload as its forced-unwind control token.
            // Preserve it exactly; cleanup payloads were separately captured and observed.
            drop(cleanup);
            std::panic::resume_unwind(payload)
        }
        Ok(value) => {
            if let Some(failure) = cleanup {
                std::panic::resume_unwind(Box::new(failure));
            }
            value
        }
    }
}

#[cfg(test)]
#[path = "scoped_test.rs"]
mod scoped_test;
