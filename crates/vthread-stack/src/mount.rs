//! One carrier-local mount for suspension and runtime-owned typed context.

use std::{cell::Cell, marker::PhantomData, ptr, sync::atomic::AtomicU8};

use crate::{Resume, SuspendError, Suspension, context::FiberCore, engine};

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

pub(crate) struct ContextSlot<'context> {
    key: *const (),
    value: *const (),
    lifetime: PhantomData<&'context ()>,
}

impl<'context> ContextSlot<'context> {
    pub(crate) fn new<T>(key: &'static ContextKey<T>, value: &'context T) -> Self {
        Self {
            key: ptr::from_ref(key).cast(),
            value: ptr::from_ref(value).cast(),
            lifetime: PhantomData,
        }
    }
}

#[derive(Clone, Copy)]
struct CurrentMount {
    core: *const FiberCore,
    context: *const ContextSlot<'static>,
}

impl CurrentMount {
    const EMPTY: Self = Self {
        core: ptr::null(),
        context: ptr::null(),
    };
}

thread_local! {
    static CURRENT_MOUNT: Cell<CurrentMount> = const { Cell::new(CurrentMount::EMPTY) };
}

/// Mounts one fiber's control block and optional typed context for one resume.
pub(crate) struct MountGuard<'mount> {
    previous: CurrentMount,
    lifetime: PhantomData<&'mount ()>,
}

impl MountGuard<'_> {
    #[inline]
    pub(crate) fn install<'mount>(
        core: *const FiberCore,
        context: Option<&'mount ContextSlot<'_>>,
    ) -> MountGuard<'mount> {
        let mounted = CurrentMount {
            core,
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
    #[inline]
    fn drop(&mut self) {
        CURRENT_MOUNT.with(|current| current.set(self.previous));
    }
}

/// Mounts only a control block, leaving any typed context in place; used for reclamation.
pub(crate) struct CoreMount {
    previous: *const FiberCore,
}

impl CoreMount {
    pub(crate) fn install(core: *const FiberCore) -> Self {
        let previous = CURRENT_MOUNT.with(|current| {
            let mut mounted = current.get();
            let previous = mounted.core;
            mounted.core = core;
            current.set(mounted);
            previous
        });
        Self { previous }
    }
}

impl Drop for CoreMount {
    fn drop(&mut self) {
        CURRENT_MOUNT.with(|current| {
            let mut mounted = current.get();
            mounted.core = self.previous;
            current.set(mounted);
        });
    }
}

pub(crate) fn mounted_core() -> *const FiberCore {
    CURRENT_MOUNT.with(|current| current.get().core)
}

/// Suspends the currently mounted fiber.
#[inline]
pub fn suspend(reason: Suspension) -> Result<Resume, SuspendError> {
    let core = mounted_core();
    if core.is_null() {
        return Err(SuspendError);
    }
    // The pointer is carrier-local and restored before leaving this mount.
    // SAFETY: it belongs to the currently mounted, non-Send execution.
    unsafe { Ok(engine::suspend(core, reason)) }
}

#[cfg(test)]
#[path = "mount_test.rs"]
mod mount_test;
