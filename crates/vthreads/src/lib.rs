//! Compatibility alias for [`vthread`].
//!
//! New applications should depend on `vthread` directly. This crate re-exports its public
//! API without adding another runtime or a separate API.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub use vthread::*;

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
