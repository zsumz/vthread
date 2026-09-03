//! One carrier-local mount for suspension and runtime-owned typed context.

use std::{cell::Cell, marker::PhantomData, ptr, sync::atomic::AtomicU8};

use corosensei::Yielder;

use crate::fiber::{Resume, SuspendError, Suspension};

pub(super) type RawYielder = Yielder<Resume, Suspension>;

/// A typed identity for context supplied while a fiber is mounted.
///
/// This is a private runtime integration hook and has no compatibility contract.
#[doc(hidden)]
pub struct ContextKey<T> {
    // Interior mutability gives every static key distinct, non-mergeable storage.
    _identity: AtomicU8,
    // Invariance prevents one key from being coerced to a different context type.
    marker: PhantomData<fn(T) -> T>,
}

impl<T> ContextKey<T> {
    /// Creates a unique context identity when stored in a static.
    #[doc(hidden)]
    pub const fn new() -> Self {
        Self {
            _identity: AtomicU8::new(0),
            marker: PhantomData,
        }
    }

    /// Runs `body` with the matching context from the current fiber mount.
    #[doc(hidden)]
    #[inline]
    pub fn with<R>(&'static self, body: impl for<'context> FnOnce(&'context T) -> R) -> Option<R> {
        CURRENT_MOUNT.with(|current| {
            let context = current.get().context;
            if context.is_null() {
                return None;
            }
            // SAFETY: the mount guard keeps its stack-local slot alive while installed.
            let slot = unsafe { &*context };
            if slot.key != ptr::from_ref(self).cast() {
                return None;
            }
            // SAFETY: ContextSlot::new paired this key with a live shared reference to T.
            // The higher-ranked callback prevents that reference from escaping this call.
            Some(body(unsafe { &*slot.value.cast::<T>() }))
        })
    }
}

impl<T> Default for ContextKey<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) struct ContextSlot<'context> {
    key: *const (),
    value: *const (),
    lifetime: PhantomData<&'context ()>,
}

impl<'context> ContextSlot<'context> {
    pub(super) fn new<T>(key: &'static ContextKey<T>, value: &'context T) -> Self {
        Self {
            key: ptr::from_ref(key).cast(),
            value: ptr::from_ref(value).cast(),
            lifetime: PhantomData,
        }
    }
}

#[derive(Clone, Copy)]
struct CurrentMount {
    yielder: *const RawYielder,
    context: *const ContextSlot<'static>,
}

impl CurrentMount {
    const EMPTY: Self = Self {
        yielder: ptr::null(),
        context: ptr::null(),
    };
}

thread_local! {
    static CURRENT_MOUNT: Cell<CurrentMount> = const { Cell::new(CurrentMount::EMPTY) };
}

pub(super) struct MountGuard<'mount> {
    previous: CurrentMount,
    lifetime: PhantomData<&'mount ()>,
}

impl MountGuard<'_> {
    pub(super) fn install<'mount>(
        yielder: *const RawYielder,
        context: Option<&'mount ContextSlot<'_>>,
    ) -> MountGuard<'mount> {
        let mounted = CurrentMount {
            yielder,
            context: context.map_or(ptr::null(), |slot| ptr::from_ref(slot).cast()),
        };
        let previous = CURRENT_MOUNT.with(|current| current.replace(mounted));
        MountGuard {
            previous,
            lifetime: PhantomData,
        }
    }
}

impl Drop for MountGuard<'_> {
    fn drop(&mut self) {
        CURRENT_MOUNT.with(|current| current.set(self.previous));
    }
}

pub(super) struct YielderMount {
    previous: *const RawYielder,
}

impl YielderMount {
    pub(super) fn install(yielder: *const RawYielder) -> Self {
        let previous = CURRENT_MOUNT.with(|current| {
            let mut mounted = current.get();
            let previous = mounted.yielder;
            mounted.yielder = yielder;
            current.set(mounted);
            previous
        });
        Self { previous }
    }
}

impl Drop for YielderMount {
    fn drop(&mut self) {
        CURRENT_MOUNT.with(|current| {
            let mut mounted = current.get();
            mounted.yielder = self.previous;
            current.set(mounted);
        });
    }
}

pub(super) fn mounted_yielder() -> *const RawYielder {
    CURRENT_MOUNT.with(|current| current.get().yielder)
}

/// Suspends the currently mounted fiber.
pub fn suspend(reason: Suspension) -> Result<Resume, SuspendError> {
    let pointer = mounted_yielder();
    if pointer.is_null() {
        return Err(SuspendError);
    }

    // The pointer is carrier-local and restored before leaving this mount.
    // SAFETY: it belongs to the currently mounted, non-Send coroutine.
    unsafe { Ok((&*pointer).suspend(reason)) }
}

#[cfg(test)]
#[path = "mount_test.rs"]
mod mount_test;
