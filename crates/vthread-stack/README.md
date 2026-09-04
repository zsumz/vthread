# vthread-stack

The stack and context-switching backend used by `vthread`.

Applications should depend on `vthread`, whose public crate forbids unsafe Rust. This package
contains the narrow stack boundary: it owns the guard-page-backed stack mappings, the native
context switch for Linux x86_64 and macOS ARM64, and the rule that every started stack is
resumed, unwound, and reclaimed by its owning carrier. For one release candidate the interim
corosensei engine remains available behind `--cfg vthread_stack_engine="corosensei"` so both
engines can be qualified against the same suite.

This support crate is published so `vthread` can be installed from crates.io. Applications
should not depend on it directly. It is licensed under the Apache License 2.0.
