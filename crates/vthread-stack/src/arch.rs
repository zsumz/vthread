//! Architecture-specific context switching for the native engine.
//!
//! Every port provides the same three operations: fabricate a fiber's first saved
//! context, switch between two saved contexts, and switch away from a finished one.
//! Lifecycle decisions never live here. Ports are declared unconditionally and gate
//! their own items, so every file binds on every target.

mod aarch64_macos;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub(crate) use aarch64_macos::{context_finish, context_switch, init_frame};

/// Targets without a port keep the engine compiling but can never select it; the
/// crate root rejects that selection at compile time, so these are unreachable.
///
/// # Safety
///
/// Never called; see the port of the same name for the real contract.
#[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
pub(crate) unsafe fn context_switch(_save_current: *mut usize, _restore: usize) {
    unreachable!("the native vthread-stack engine has no port for this target")
}

/// # Safety
///
/// Never called; see the port of the same name for the real contract.
#[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
pub(crate) unsafe fn context_finish(_restore: usize) -> ! {
    unreachable!("the native vthread-stack engine has no port for this target")
}

/// # Safety
///
/// Never called; see the port of the same name for the real contract.
#[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
pub(crate) unsafe fn init_frame(
    _stack: &crate::MappedStack,
    _core: std::ptr::NonNull<crate::context::FiberCore>,
) -> usize {
    unreachable!("the native vthread-stack engine has no port for this target")
}

#[cfg(test)]
#[path = "arch_test.rs"]
mod arch_test;
