# vthread-stack

The stack and context-switching backend used by `vthread`.

Applications should depend on `vthread`, whose public crate forbids unsafe Rust. This package
contains the narrow stack boundary, pins corosensei, and enforces the rule that every started
stack is resumed, unwound, and reclaimed by its owning carrier.

This support crate is published so `vthread` can be installed from crates.io. Applications
should not depend on it directly. It is licensed under the Apache License 2.0.
