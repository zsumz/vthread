# vthread-stack

The stack and context-switching backend used by `vthread`.

Applications should depend on `vthread`, whose public crate forbids unsafe Rust. This package
contains the narrow stack boundary: it owns the guard-page-backed stack mappings, the context
switch for Linux x86_64 and macOS ARM64, and the rule that every started stack is resumed,
unwound, and reclaimed by its owning carrier. A fiber's control block and entry live at the
top of its own stack, so starting one on a pooled stack allocates nothing.

This support crate is published so `vthread` can be installed from crates.io. Applications
should not depend on it directly. It is licensed under the Apache License 2.0.
