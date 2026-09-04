//! Safe exclusive value ownership across virtual suspension.

use super::wait::Wait;
use crate::{
    Error, Parker, Result, SuspensionReason,
    signal::lock,
    wait::{ResourceSelection, WaitCell},
};
use std::{
    collections::VecDeque,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::{
        Mutex as NativeMutex, MutexGuard as NativeMutexGuard,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

struct MutexGate {
    locked: AtomicBool,
    outstanding: AtomicUsize,
    capacity: usize,
    entries: NativeMutex<VecDeque<WaitCell>>,
}

struct Ticket<'a> {
    gate: &'a MutexGate,
    parker: Option<Parker>,
}

/// A FIFO virtual mutex whose gate keeps the native value lock uncontended.
///
/// The native guard stays on the task's immutable owner carrier while user code
/// runs or suspends. This mutex does not expose poisoning: unwinding releases the
/// possibly modified value. Repair application invariants explicitly if a panic
/// can leave them incomplete. It is not reentrant.
pub struct Mutex<T> {
    value: NativeMutex<T>,
    gate: MutexGate,
}

/// Exclusive access that stays on the current carrier and unlocks on drop.
/// Forgetting a guard leaks the value and permanently retains the lock.
///
/// ```compile_fail
/// let mutex = vthread::sync::Mutex::new(42);
/// let guard = mutex.try_lock().unwrap();
/// std::thread::scope(|scope| { scope.spawn(move || drop(guard)); });
/// ```
#[must_use = "dropping the guard immediately unlocks the mutex"]
pub struct MutexGuard<'a, T> {
    pub(super) mutex: &'a Mutex<T>,
    value: Option<NativeMutexGuard<'a, T>>,
    _affine: PhantomData<Rc<()>>,
}

impl<T> Mutex<T> {
    /// Creates an unlocked mutex using [`super::DEFAULT_WAIT_CAPACITY`].
    pub fn new(value: T) -> Self {
        Self::with_wait_capacity(value, super::DEFAULT_WAIT_CAPACITY)
            .expect("default waiter capacity is positive")
    }

    /// Creates an unlocked mutex with an explicit positive waiter limit.
    pub fn with_wait_capacity(value: T, wait_capacity: usize) -> Result<Self> {
        Ok(Self {
            value: NativeMutex::new(value),
            gate: MutexGate::new(wait_capacity)?,
        })
    }

    /// Locks the value, parking the current virtual thread under contention.
    pub fn lock(&self) -> Result<MutexGuard<'_, T>> {
        self.gate.acquire(SuspensionReason::Mutex)?;
        Ok(self.guard())
    }

    /// Locks immediately or returns `Error::WouldBlock`; also usable by OS callers.
    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>> {
        self.gate.try_acquire()?;
        Ok(self.guard())
    }

    fn guard(&self) -> MutexGuard<'_, T> {
        MutexGuard {
            mutex: self,
            value: Some(lock(&self.value)),
            _affine: PhantomData,
        }
    }

    /// Number of outstanding lock waits, including selected waiters.
    pub fn waiting(&self) -> usize {
        self.gate.waiting()
    }

    /// Configured outstanding-wait limit, including selected waiters.
    pub fn wait_capacity(&self) -> usize {
        self.gate.wait_capacity()
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_deref().expect("live mutex guard")
    }
}
impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_deref_mut().expect("live mutex guard")
    }
}
impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        // Release the value before selecting the next logical owner.
        drop(self.value.take());
        self.mutex.gate.release();
    }
}

impl MutexGate {
    fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::WaitCapacity,
                "must be positive",
            ));
        }
        Ok(Self {
            locked: AtomicBool::new(false),
            outstanding: AtomicUsize::new(0),
            capacity,
            entries: NativeMutex::new(VecDeque::new()),
        })
    }

    fn acquire(&self, reason: SuspensionReason) -> Result<()> {
        crate::context::check_current()?;
        if self.try_take() {
            return Ok(());
        }
        let wait = Wait::enter_after_check(reason)?;
        let parker = wait.parker()?;
        let Some(ticket) = Ticket::subscribe(self, parker)? else {
            return Ok(());
        };
        ticket.wait(&wait)
    }

    fn try_acquire(&self) -> Result<()> {
        self.try_take().then_some(()).ok_or(Error::WouldBlock)
    }

    fn release(&self) {
        loop {
            let entry = {
                let mut entries = lock(&self.entries);
                let Some(entry) = entries.pop_front() else {
                    #[cfg(debug_assertions)]
                    assert!(self.locked.load(Ordering::Relaxed));
                    self.locked.store(false, Ordering::Release);
                    return;
                };
                entry
            };
            if entry.offer_resource(ResourceSelection::Permit) {
                return;
            }
        }
    }

    fn waiting(&self) -> usize {
        self.outstanding.load(Ordering::Acquire)
    }

    const fn wait_capacity(&self) -> usize {
        self.capacity
    }

    fn try_take(&self) -> bool {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    fn reserve(&self) -> Result<()> {
        if self
            .outstanding
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |outstanding| {
                (outstanding < self.capacity).then_some(outstanding + 1)
            })
            .is_err()
        {
            return Err(Error::Capacity {
                resource: crate::error::CapacityResource::Waiters,
                limit: self.capacity,
            });
        }
        Ok(())
    }

    fn retire(&self) {
        let previous = self.outstanding.fetch_sub(1, Ordering::AcqRel);
        assert!(previous != 0, "mutex ticket released twice");
    }
}

impl<'a> Ticket<'a> {
    fn subscribe(gate: &'a MutexGate, parker: Parker) -> Result<Option<Self>> {
        gate.reserve()?;
        let mut entries = lock(&gate.entries);
        if gate.try_take() {
            gate.retire();
            return Ok(None);
        }
        entries.push_back(parker.wait.clone());
        Ok(Some(Self {
            gate,
            parker: Some(parker),
        }))
    }

    fn wait(mut self, wait: &Wait) -> Result<()> {
        wait.park(self.parker())?;
        self.complete();
        Ok(())
    }

    fn parker(&self) -> &Parker {
        self.parker.as_ref().expect("live mutex ticket")
    }

    fn complete(&mut self) {
        drop(self.parker.take().expect("live mutex ticket"));
        self.gate.retire();
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        let Some(parker) = self.parker.take() else {
            return;
        };
        let handed_off = {
            let mut entries = lock(&self.gate.entries);
            match entries
                .iter()
                .position(|entry| entry.same_cell(&parker.wait))
            {
                Some(index) => {
                    drop(entries.remove(index).expect("mutex ticket position"));
                    false
                }
                None => {
                    drop(entries);
                    parker.wait.take_resource() == Some(ResourceSelection::Permit)
                }
            }
        };
        self.gate.retire();
        if handed_off {
            self.gate.release();
        }
    }
}

#[cfg(test)]
#[path = "mutex_test.rs"]
mod mutex_test;
