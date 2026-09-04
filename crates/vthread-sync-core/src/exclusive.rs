//! Linear ownership capabilities around one exclusively accessed value.

use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};

const LOCKED: usize = 1;
const HAS_WAITERS: usize = 1 << 1;
static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);

/// A value whose unique access can be transferred without unlocking it.
pub struct ExclusiveCell<T> {
    state: AtomicUsize,
    identity: u64,
    value: UnsafeCell<T>,
}

/// Unique access to an [`ExclusiveCell`].
pub struct ExclusiveGuard<'a, T> {
    cell: &'a ExclusiveCell<T>,
    _affine: PhantomData<Rc<()>>,
}

/// A linear, non-forgeable ownership handoff between guards.
pub struct Ownership {
    identity: u64,
}

/// A one-entry mailbox for transferring an [`Ownership`] capability.
///
/// Publishing consumes the capability, and taking reconstructs it exactly once. An empty slot is
/// represented by zero, which is never assigned as an [`ExclusiveCell`] identity.
pub struct OwnershipSlot {
    identity: AtomicU64,
}

/// Result of closing the enqueue-versus-unlock race under an external queue lock.
pub enum QueueDecision<'a, T> {
    /// The value became available before the caller joined the queue.
    Acquired(ExclusiveGuard<'a, T>),
    /// The current owner observed or will observe the waiter marker.
    Queued,
}

impl OwnershipSlot {
    /// Creates an empty ownership mailbox.
    pub const fn new() -> Self {
        Self {
            identity: AtomicU64::new(0),
        }
    }

    /// Publishes one capability, returning it unchanged if the mailbox is occupied.
    pub fn publish(&self, ownership: Ownership) -> Result<(), Ownership> {
        match self.identity.compare_exchange(
            0,
            ownership.identity,
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => Ok(()),
            Err(_) => Err(ownership),
        }
    }

    /// Takes the published capability exactly once.
    pub fn take(&self) -> Option<Ownership> {
        let identity = self.identity.swap(0, Ordering::Acquire);
        (identity != 0).then_some(Ownership { identity })
    }
}

impl Default for OwnershipSlot {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: `state` and the linear `Ownership` capability permit at most one live guard.
// A guard is carrier-affine, and sharing the cell can only transfer a `T: Send` value.
unsafe impl<T: Send> Sync for ExclusiveCell<T> {}

impl<T> ExclusiveCell<T> {
    /// Creates an initially unlocked cell.
    pub fn new(value: T) -> Self {
        let identity = NEXT_IDENTITY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |identity| {
                identity.checked_add(1)
            })
            .expect("exclusive cell identity exhausted");
        Self {
            state: AtomicUsize::new(0),
            identity,
            value: UnsafeCell::new(value),
        }
    }

    /// Acquires an unlocked cell without bypassing a published waiter marker.
    pub fn try_lock(&self) -> Option<ExclusiveGuard<'_, T>> {
        self.state
            .compare_exchange(0, LOCKED, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| self.guard())
    }

    /// Acquires the cell or publishes contention atomically with owner release.
    ///
    /// Callers serialize this operation with their bounded waiter queue. `Queued` means the
    /// caller must publish exactly one queue entry before releasing that external queue lock.
    pub fn queue_or_lock(&self) -> QueueDecision<'_, T> {
        let mut state = self.state.load(Ordering::Acquire);
        loop {
            if state == 0 {
                match self.state.compare_exchange_weak(
                    0,
                    LOCKED,
                    Ordering::Acquire,
                    Ordering::Acquire,
                ) {
                    Ok(_) => return QueueDecision::Acquired(self.guard()),
                    Err(observed) => state = observed,
                }
                continue;
            }
            assert!(state & LOCKED != 0, "waiter marker exists without an owner");
            if state & HAS_WAITERS != 0 {
                return QueueDecision::Queued;
            }
            match self.state.compare_exchange_weak(
                state,
                state | HAS_WAITERS,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return QueueDecision::Queued,
                Err(observed) => state = observed,
            }
        }
    }

    /// Converts the exact handoff capability back into unique value access.
    pub fn claim(&self, ownership: Ownership) -> Result<ExclusiveGuard<'_, T>, Ownership> {
        if ownership.identity != self.identity || self.state.load(Ordering::Acquire) & LOCKED == 0 {
            return Err(ownership);
        }
        Ok(self.guard())
    }

    /// Publishes whether queue entries remain while retaining logical ownership.
    pub fn set_waiters(&self, ownership: &Ownership, has_waiters: bool) -> bool {
        if ownership.identity != self.identity || self.state.load(Ordering::Acquire) & LOCKED == 0 {
            return false;
        }
        self.state.store(
            LOCKED | (usize::from(has_waiters) * HAS_WAITERS),
            Ordering::Release,
        );
        true
    }

    /// Releases an unclaimed handoff capability.
    pub fn unlock(&self, ownership: Ownership) -> Result<(), Ownership> {
        if ownership.identity != self.identity || self.state.load(Ordering::Acquire) & LOCKED == 0 {
            return Err(ownership);
        }
        self.state.store(0, Ordering::Release);
        Ok(())
    }

    fn guard(&self) -> ExclusiveGuard<'_, T> {
        ExclusiveGuard {
            cell: self,
            _affine: PhantomData,
        }
    }
}

impl<T> ExclusiveGuard<'_, T> {
    /// Releases immediately when uncontended or returns ownership for queued handoff.
    pub fn try_release(self) -> Result<(), Ownership> {
        let cell = self.cell;
        std::mem::forget(self);
        match cell
            .state
            .compare_exchange(LOCKED, 0, Ordering::Release, Ordering::Relaxed)
        {
            Ok(_) => Ok(()),
            Err(state) => {
                assert!(state & LOCKED != 0, "exclusive guard lost ownership");
                Err(Ownership {
                    identity: cell.identity,
                })
            }
        }
    }
}

impl<T> Deref for ExclusiveGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: construction requires the cell's unique live guard capability.
        unsafe { &*self.cell.value.get() }
    }
}

impl<T> DerefMut for ExclusiveGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the linear guard is the only capability that can access the value.
        unsafe { &mut *self.cell.value.get() }
    }
}

impl<T> Drop for ExclusiveGuard<'_, T> {
    fn drop(&mut self) {
        let previous = self.cell.state.swap(0, Ordering::Release);
        assert!(previous & LOCKED != 0, "exclusive guard released twice");
    }
}

#[cfg(test)]
#[path = "exclusive_test.rs"]
mod exclusive_test;
