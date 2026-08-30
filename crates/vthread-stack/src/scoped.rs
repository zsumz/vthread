//! Borrowed fibers whose executable state is revoked before their lexical scope exits.

use crate::{Fiber, FiberState};
use corosensei::stack::DefaultStack;
use std::{
    cell::{Cell, RefCell},
    io,
    marker::PhantomData,
    rc::Rc,
};

/// A carrier-local lease. After its scope exits it contains no executable stack.
#[derive(Clone)]
pub struct FiberLease(Rc<RefCell<LeaseState>>);

struct LeaseState {
    fiber: Option<Fiber>,
    cleanup: Option<Rc<dyn Fn() -> Box<dyn std::any::Any>>>,
}

impl FiberLease {
    /// Resumes a live stack; None means the scope has already reclaimed it.
    pub fn resume(&self) -> Option<FiberState> {
        self.0.borrow_mut().fiber.as_mut().map(Fiber::resume)
    }

    /// Takes a completed stack for reuse.
    pub fn take_stack(&self) -> Option<DefaultStack> {
        let (fiber, cleanup) = {
            let mut state = self.0.borrow_mut();
            (state.fiber.take(), state.cleanup.take())
        };
        drop(cleanup);
        fiber.map(Fiber::into_stack)
    }

    /// Reclaims a suspended or unstarted stack on its owner thread.
    pub fn reclaim(&self) {
        let (fiber, cleanup) = {
            let mut state = self.0.borrow_mut();
            (state.fiber.take(), state.cleanup.take())
        };
        if fiber.is_some() {
            let _context = cleanup.map(|cleanup| cleanup());
            drop(fiber);
        }
    }

    /// Whether this lease still owns an executable stack.
    pub fn live(&self) -> bool {
        self.0.borrow().fiber.is_some()
    }

    /// Installs a carrier-local guard around forced stack destruction.
    pub fn cleanup_context(&self, mount: impl Fn() -> Box<dyn std::any::Any> + 'static) {
        self.0.borrow_mut().cleanup = Some(Rc::new(mount));
    }
}

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
        stack: DefaultStack,
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
        let lease = FiberLease(Rc::new(RefCell::new(LeaseState {
            fiber: Some(fiber),
            cleanup: None,
        })));
        fibers.push(lease.clone());
        Ok(lease)
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        self.0.closed.set(true);
        let fibers = self.0.fibers.take();
        // A destructor panic cannot let subsequent borrowed stacks escape reclamation.
        let mut panic = None;
        for fiber in fibers {
            if let Err(payload) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fiber.reclaim()))
            {
                panic.get_or_insert(payload);
            }
        }
        if let Some(payload) = panic {
            std::panic::resume_unwind(payload);
        }
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
    let _guard = ScopeGuard(Rc::clone(&scope.registry));
    body(&scope)
}

#[cfg(test)]
#[path = "scoped_test.rs"]
mod scoped_test;
