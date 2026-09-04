//! Architecture-specific context switching for the native engine.
//!
//! Every port provides the same three operations: fabricate a fiber's first saved
//! context, switch between two saved contexts, and switch away from a finished one.
//! Lifecycle decisions never live here. Ports are declared unconditionally and gate
//! their own items, so every file binds on every target.

mod aarch64_macos;
mod x86_64_sysv;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub(crate) use aarch64_macos::{context_finish, context_switch, init_frame};
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub(crate) use x86_64_sysv::{context_finish, context_switch, init_frame};

#[cfg(test)]
#[path = "arch_test.rs"]
mod arch_test;
