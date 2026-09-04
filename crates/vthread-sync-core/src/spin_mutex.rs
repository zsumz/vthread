//! Poison-free mutual exclusion for very short nonblocking critical sections.

use std::{
    cell::UnsafeCell,
    hint::spin_loop,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

const SPINS_BEFORE_YIELD: usize = 64;

/// A mutex for short critical sections that never park an operating-system thread.
pub struct SpinMutex<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

/// Unique access to a [`SpinMutex`], released without poisoning on drop.
pub struct SpinMutexGuard<'a, T> {
    mutex: &'a SpinMutex<T>,
    _affine: PhantomData<Rc<()>>,
}

// SAFETY: `locked` admits one guard, and the guard is the only path to `value`.
// Moving the mutex between threads is sound exactly when moving `T` is sound.
unsafe impl<T: Send> Sync for SpinMutex<T> {}

impl<T> SpinMutex<T> {
    /// Creates an unlocked mutex.
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Acquires the mutex, yielding the current operating-system thread after bounded spin batches.
    pub fn lock(&self) -> SpinMutexGuard<'_, T> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            self.lock_slow();
        }
        SpinMutexGuard {
            mutex: self,
            _affine: PhantomData,
        }
    }

    #[cold]
    fn lock_slow(&self) {
        loop {
            for _ in 0..SPINS_BEFORE_YIELD {
                if !self.locked.load(Ordering::Relaxed)
                    && self
                        .locked
                        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                        .is_ok()
                {
                    return;
                }
                spin_loop();
            }
            std::thread::yield_now();
        }
    }
}

impl<T> Deref for SpinMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: construction follows the unique successful lock acquisition.
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for SpinMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the unique live guard is the only mutable access capability.
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T> Drop for SpinMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.locked.store(false, Ordering::Release);
    }
}

#[cfg(test)]
#[path = "spin_mutex_test.rs"]
mod spin_mutex_test;
