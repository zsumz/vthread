//! Architecture-specific context switching.
//!
//! Every port provides the same three operations: fabricate a fiber's first saved
//! context below an aligned address, switch between two saved contexts, and switch away
//! from a finished one. Lifecycle decisions never live here. Ports are declared
//! unconditionally and gate their own items, so every file binds on every target.

mod aarch64_macos;
mod x86_64_sysv;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub(crate) use aarch64_macos::{
    FRAME_LEN, context_finish, context_switch as context_resume, context_switch as context_suspend,
    init_frame,
};
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub(crate) use x86_64_sysv::{
    FRAME_LEN, context_finish, context_resume, context_suspend, init_frame,
};

#[cfg(test)]
#[path = "arch_test.rs"]
mod arch_test;
