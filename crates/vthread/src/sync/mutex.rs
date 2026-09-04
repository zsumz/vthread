//! Safe exclusive value ownership across virtual suspension.

use super::wait::Wait;
use crate::{
    Error, Result, SuspensionReason,
    signal::lock,
    wait::{ResourceSelection, WaitCell},
};
use std::{
    cell::RefMut,
    collections::VecDeque,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::{
        Mutex as NativeMutex, MutexGuard as NativeMutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
};

struct MutexGate {
    state: AtomicUsize,
    outstanding: AtomicUsize,
    capacity: usize,
    entries: NativeMutex<VecDeque<WaitCell>>,
}

struct Ticket<'gate, 'wait> {
    gate: &'gate MutexGate,
    wait: Option<RefMut<'wait, WaitCell>>,
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
    const LOCKED: usize = 1;
    const HAS_WAITERS: usize = 1 << 1;

    fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(Error::invalid_configuration(
                crate::error::ConfigurationField::WaitCapacity,
                "must be positive",
            ));
        }
        Ok(Self {
            state: AtomicUsize::new(0),
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
        let synchronization_wait = wait.synchronization_wait()?;
        let Some(ticket) = Ticket::subscribe(self, synchronization_wait)? else {
            return Ok(());
        };
        ticket.wait(&wait)
    }

    fn try_acquire(&self) -> Result<()> {
        self.try_take().then_some(()).ok_or(Error::WouldBlock)
    }

    fn release(&self) {
        if self.state.load(Ordering::Acquire) == Self::LOCKED
            && self
                .state
                .compare_exchange(Self::LOCKED, 0, Ordering::Release, Ordering::Relaxed)
                .is_ok()
        {
            return;
        }
        loop {
            let entry = {
                let mut entries = lock(&self.entries);
                let Some(entry) = entries.pop_front() else {
                    self.state.store(0, Ordering::Release);
                    return;
                };
                if entries.is_empty() {
                    self.state.store(Self::LOCKED, Ordering::Release);
                }
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
        self.state
            .compare_exchange(0, Self::LOCKED, Ordering::Acquire, Ordering::Relaxed)
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

impl<'gate, 'wait> Ticket<'gate, 'wait> {
    fn subscribe(gate: &'gate MutexGate, wait: RefMut<'wait, WaitCell>) -> Result<Option<Self>> {
        gate.reserve()?;
        let mut entries = lock(&gate.entries);
        let previous = gate
            .state
            .fetch_or(MutexGate::HAS_WAITERS, Ordering::AcqRel);
        if previous == 0 {
            gate.state.store(MutexGate::LOCKED, Ordering::Release);
            gate.retire();
            return Ok(None);
        }
        assert!(
            previous & MutexGate::LOCKED != 0,
            "mutex waiter bit existed without an owner"
        );
        entries.push_back(wait.clone());
        Ok(Some(Self {
            gate,
            wait: Some(wait),
        }))
    }

    fn wait(mut self, wait: &Wait) -> Result<()> {
        wait.park_wait(self.wait_cell())?;
        self.complete();
        Ok(())
    }

    fn wait_cell(&self) -> &WaitCell {
        self.wait.as_deref().expect("live mutex ticket")
    }

    fn complete(&mut self) {
        drop(self.wait.take().expect("live mutex ticket"));
        self.gate.retire();
    }
}

impl Drop for Ticket<'_, '_> {
    fn drop(&mut self) {
        let Some(wait) = self.wait.take() else {
            return;
        };
        let (queued, selection) = {
            let mut entries = lock(&self.gate.entries);
            match entries.iter().position(|entry| entry.same_cell(&wait)) {
                Some(index) => {
                    drop(entries.remove(index).expect("mutex ticket position"));
                    if entries.is_empty() {
                        self.gate.state.store(MutexGate::LOCKED, Ordering::Release);
                    }
                    (true, None)
                }
                None => {
                    drop(entries);
                    (false, wait.take_resource())
                }
            }
        };
        self.gate.retire();
        if !queued && selection == Some(ResourceSelection::Permit) {
            self.gate.release();
        }
    }
}

#[cfg(test)]
#[path = "mutex_test.rs"]
mod mutex_test;
